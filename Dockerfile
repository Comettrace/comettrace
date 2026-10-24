FROM --platform=$BUILDPLATFORM rust:1-slim-bookworm AS build

ARG TARGETARCH
ARG FEATURES=""

WORKDIR /src
COPY . .

RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) \
        target=x86_64-unknown-linux-gnu; \
        cross=""; \
        ;; \
      arm64) \
        target=aarch64-unknown-linux-gnu; \
        cross="gcc-aarch64-linux-gnu libc6-dev-arm64-cross"; \
        ;; \
      *) echo "no build for $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    if [ -n "$cross" ]; then \
      apt-get update; \
      apt-get install --yes --no-install-recommends $cross; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    rustup target add "$target"; \
    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc; \
    export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc; \
    cargo build --release --locked --package comettrace --target "$target" \
      ${FEATURES:+--features "$FEATURES"}; \
    cp "target/$target/release/comettrace" /comettrace

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=build /comettrace /usr/local/bin/comettrace

ENV PATH=/usr/local/bin:/usr/bin:/bin
ENV RUST_LOG=info
EXPOSE 4317 4318

ENTRYPOINT ["/usr/local/bin/comettrace"]
