FROM rust:1-slim-bookworm AS build

WORKDIR /src
COPY . .
RUN cargo build --release --locked --package comettrace

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --shell /usr/sbin/nologin comettrace

COPY --from=build /src/target/release/comettrace /usr/local/bin/comettrace

USER comettrace
WORKDIR /home/comettrace
EXPOSE 4317 4318

ENV RUST_LOG=info

ENTRYPOINT ["comettrace"]
