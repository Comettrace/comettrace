use core::fmt;
use core::future::Future;

use crate::identity::AccountId;
use crate::tenant::TenantId;

/* What the screen promised before anything backed it -- "PNG or JPG, square, up to 1 MB" -- and
what an avatar actually needs: low enough that a photo is never the reason a page is slow, high
enough for a picture somebody cropped on a phone. Nothing is re-encoded on the way in, because a
ceiling this low makes a resizer a dependency rather than a saving. */
pub const MAX_PHOTO_BYTES: usize = 1024 * 1024;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const JPEG: &[u8] = &[0xff, 0xd8, 0xff];
const RIFF: &[u8] = b"RIFF";
const WEBP: &[u8] = b"WEBP";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoKind {
    Png,
    Jpeg,
    Webp,
}

impl PhotoKind {
    /* The media type is the spelling, because it is what the column holds and what the answer
    carries: one name rather than a short one beside a header value. */
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    pub fn named(spelling: &str) -> Option<Self> {
        match spelling {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            "image/webp" => Some(Self::Webp),
            _ => None,
        }
    }

    /* Sniffed from the first bytes rather than believed from the request. A content type is what
    a sender claims, and an avatar that is really a script is a file we would hand back to every
    reader's browser with our own origin on it. */
    fn sniffed(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(PNG) {
            return Some(Self::Png);
        }
        if bytes.starts_with(JPEG) {
            return Some(Self::Jpeg);
        }
        if bytes.starts_with(RIFF) && bytes.len() > 12 && &bytes[8..12] == WEBP {
            return Some(Self::Webp);
        }
        None
    }
}

impl fmt::Display for PhotoKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadPhoto {
    #[error("a photo is at most {0} bytes")]
    TooLarge(usize),
    #[error("a photo must be a png, a jpeg or a webp")]
    NotAnImage,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Photo {
    kind: PhotoKind,
    bytes: Vec<u8>,
}

/* The bytes are somebody's face. Their count is what a log or a panic has any use for. */
impl fmt::Debug for Photo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Photo")
            .field("kind", &self.kind)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

impl Photo {
    /* The ceiling is checked before the bytes are sniffed, so somebody who sent a film is told
    the size rather than that a film is not a png. Empty bytes are no image: there is nothing to
    sniff, and nothing to keep. */
    pub fn examined(bytes: Vec<u8>) -> Result<Self, BadPhoto> {
        if bytes.len() > MAX_PHOTO_BYTES {
            return Err(BadPhoto::TooLarge(MAX_PHOTO_BYTES));
        }
        let kind = PhotoKind::sniffed(&bytes).ok_or(BadPhoto::NotAnImage)?;
        Ok(Self { kind, bytes })
    }

    pub fn kind(&self) -> PhotoKind {
        self.kind
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/* What the control plane keeps about a photo: what it is, and the token the store named the
object after. The token changes with every upload, so a url carrying it can be cached for ever
and still change the moment the photo does. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Portrait {
    pub token: Box<str>,
    pub kind: PhotoKind,
}

/* Whose picture this is. The store namespaces its objects by the variant, so an account and an
organisation that happen to carry the same uuid never name the same object, and a bucket can still
be read by eye. The two kinds answer the same three questions, which is why there is one port here
rather than one per kind: what differs between them is a key prefix, and the signing underneath it
is the expensive half. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pictured {
    Account(AccountId),
    Org(TenantId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PhotoError {
    #[error("this deployment keeps no photos: it has no object store configured")]
    NotOffered,
    #[error("the photo store could not be reached")]
    Unreachable,
}

/* Where the bytes live. The store names its own objects and hands back the token it used, so
nothing above it has to know how a key is spelt or where the randomness comes from. */
pub trait Photos {
    fn put(
        &self,
        of: Pictured,
        photo: &Photo,
    ) -> impl Future<Output = Result<Portrait, PhotoError>> + Send;

    fn fetch(
        &self,
        of: Pictured,
        portrait: &Portrait,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, PhotoError>> + Send;

    fn remove(
        &self,
        of: Pictured,
        portrait: &Portrait,
    ) -> impl Future<Output = Result<(), PhotoError>> + Send;
}

impl<T: Photos + Sync> Photos for &T {
    fn put(
        &self,
        of: Pictured,
        photo: &Photo,
    ) -> impl Future<Output = Result<Portrait, PhotoError>> + Send {
        (*self).put(of, photo)
    }

    fn fetch(
        &self,
        of: Pictured,
        portrait: &Portrait,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, PhotoError>> + Send {
        (*self).fetch(of, portrait)
    }

    fn remove(
        &self,
        of: Pictured,
        portrait: &Portrait,
    ) -> impl Future<Output = Result<(), PhotoError>> + Send {
        (*self).remove(of, portrait)
    }
}

impl<T: Photos + Send + Sync> Photos for std::sync::Arc<T> {
    fn put(
        &self,
        of: Pictured,
        photo: &Photo,
    ) -> impl Future<Output = Result<Portrait, PhotoError>> + Send {
        self.as_ref().put(of, photo)
    }

    fn fetch(
        &self,
        of: Pictured,
        portrait: &Portrait,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, PhotoError>> + Send {
        self.as_ref().fetch(of, portrait)
    }

    fn remove(
        &self,
        of: Pictured,
        portrait: &Portrait,
    ) -> impl Future<Output = Result<(), PhotoError>> + Send {
        self.as_ref().remove(of, portrait)
    }
}

/* A deployment with no object store configured. It refuses rather than being absent, so somebody
who presses the control is told why instead of meeting a route that is not there -- the way
NoProvider refuses a sign in nobody enabled. */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoPhotos;

impl Photos for NoPhotos {
    async fn put(&self, _of: Pictured, _photo: &Photo) -> Result<Portrait, PhotoError> {
        Err(PhotoError::NotOffered)
    }

    async fn fetch(
        &self,
        _of: Pictured,
        _portrait: &Portrait,
    ) -> Result<Option<Vec<u8>>, PhotoError> {
        Err(PhotoError::NotOffered)
    }

    async fn remove(&self, _of: Pictured, _portrait: &Portrait) -> Result<(), PhotoError> {
        Err(PhotoError::NotOffered)
    }
}

#[cfg(test)]
mod tests {
    use super::{BadPhoto, MAX_PHOTO_BYTES, Photo, PhotoKind};

    fn a_png() -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        bytes.extend_from_slice(b"and then whatever an encoder wrote");
        bytes
    }

    fn a_jpeg() -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe0];
        bytes.extend_from_slice(b"JFIF and the rest");
        bytes
    }

    fn a_webp() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0x20, 0x00, 0x00, 0x00]);
        bytes.extend_from_slice(b"WEBPVP8 and the rest");
        bytes
    }

    #[test]
    fn each_kind_is_recognised_by_its_first_bytes() {
        assert_eq!(
            Photo::examined(a_png()).expect("a png").kind(),
            PhotoKind::Png
        );
        assert_eq!(
            Photo::examined(a_jpeg()).expect("a jpeg").kind(),
            PhotoKind::Jpeg
        );
        assert_eq!(
            Photo::examined(a_webp()).expect("a webp").kind(),
            PhotoKind::Webp
        );
    }

    /* The one that matters: a sender says what it likes in a content type, and we would hand
    whatever it sent back to every reader's browser under our own origin. */
    #[test]
    fn anything_that_is_not_an_image_is_refused_however_it_is_announced() {
        assert_eq!(
            Photo::examined(b"<script>alert(1)</script>".to_vec()),
            Err(BadPhoto::NotAnImage)
        );
        assert_eq!(Photo::examined(Vec::new()), Err(BadPhoto::NotAnImage));
        assert_eq!(
            Photo::examined(b"RIFFxxxxWAVE".to_vec()),
            Err(BadPhoto::NotAnImage),
            "a riff container is not a webp unless it says so"
        );
    }

    #[test]
    fn a_photo_past_the_ceiling_is_refused_before_it_is_sniffed() {
        let mut huge = a_png();
        huge.resize(MAX_PHOTO_BYTES + 1, 0);

        assert_eq!(
            Photo::examined(huge),
            Err(BadPhoto::TooLarge(MAX_PHOTO_BYTES)),
            "the size is the answer, rather than what a film is not"
        );
    }

    #[test]
    fn a_photo_right_on_the_ceiling_is_kept() {
        let mut full = a_png();
        full.resize(MAX_PHOTO_BYTES, 0);

        assert!(Photo::examined(full).is_ok());
    }

    #[test]
    fn a_photo_never_renders_its_bytes() {
        let rendered = format!("{:?}", Photo::examined(a_png()).expect("a png"));

        assert_eq!(rendered, "Photo { kind: Png, bytes: 42 }");
    }

    /* The column holds the media type, so the spelling has to survive a round trip through it. */
    #[test]
    fn a_kind_is_read_back_from_the_spelling_it_is_stored_under() {
        for kind in [PhotoKind::Png, PhotoKind::Jpeg, PhotoKind::Webp] {
            assert_eq!(PhotoKind::named(kind.as_str()), Some(kind));
        }
        assert_eq!(PhotoKind::named("image/gif"), None);
        assert_eq!(PhotoKind::named("png"), None);
    }
}
