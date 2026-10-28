# syntax=docker/dockerfile:1

FROM --platform=$BUILDPLATFORM node:22-alpine AS web

WORKDIR /web
RUN corepack enable && corepack prepare pnpm@10 --activate
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

FROM --platform=$BUILDPLATFORM rust:1-slim-bookworm AS build

ARG TARGETARCH
ARG FEATURES=""

WORKDIR /src

# Only what cargo reads, so a change under web/ leaves this layer and its cache alone.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
COPY apps apps

# The registry and the target directory are cache mounts, so a second build compiles what changed
# rather than the whole dependency tree. The binary is copied out inside the same step: a cache
# mount is gone by the time the next one runs.
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry,sharing=locked \
    --mount=type=cache,target=/src/target,id=cargo-target-$TARGETARCH,sharing=locked \
    set -eu; \
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
COPY --from=web /web/dist /usr/local/share/comettrace/web

ENV PATH=/usr/local/bin:/usr/bin:/bin
ENV RUST_LOG=info
ENV COMETTRACE_WEB_ROOT=/usr/local/share/comettrace/web
EXPOSE 4317 4318

ENTRYPOINT ["/usr/local/bin/comettrace"]
