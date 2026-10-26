# Comettrace

Distributed tracing you can host yourself. An OTLP endpoint takes spans over
gRPC or HTTP, ClickHouse keeps them, Postgres keeps the accounts, the
organisations, the projects and the keys, and a web application reads the
traces back.

One command brings the whole thing up, and one more gives it a way in:
[deploy/README.md](deploy/README.md).

```sh
cd deploy
docker compose up --build --detach
docker compose exec server comettrace bootstrap --email you@example.com
```

The server then answers OTLP over gRPC on 4317, and OTLP over HTTP, the API
and the web application on <http://localhost:4318>.

## What is in here

| Path | What it holds |
| --- | --- |
| `crates/core` | The domain: entities, value objects, use cases, and the port traits the rest implements. Depends on no infrastructure. |
| `crates/otlp` | OTLP protobuf in, domain spans out. |
| `crates/storage` | The driven adapters: Postgres, ClickHouse, Redis, S3, SMTP, OIDC. |
| `crates/api` | The driving adapters: the HTTP router, the OTLP gRPC service, the DTOs. |
| `crates/ee` | The cloud build, behind the `ee` feature. Empty in this repository. |
| `apps/server` | The binary: configuration, wiring, the CLI, the server. |
| `web` | The front end: React, Vite, TypeScript. |
| `deploy` | The self-hosted stack and its guide. |
| `docs/adr` | Every architecture decision, in the order it was taken. |

Dependencies point inward: `crates/api` and `crates/storage` know
`crates/core`, `crates/core` knows neither. `apps/server` is the only place
that names a concrete adapter.

## Working on it

Rust 1.97.0, edition 2024, pinned in `rust-toolchain.toml`. Node 22 and
pnpm 10 for the front end.

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && pnpm install && pnpm typecheck
```

The integration tests want a real backend and are skipped without one. Set
`REQUIRE_DATABASE_URL`, `REQUIRE_CLICKHOUSE_URL` or `REQUIRE_REDIS_URL` to
turn a missing backend into a failure instead of a skip:

```sh
DATABASE_URL=postgres://comettrace:comettrace@localhost:5432/comettrace \
REQUIRE_DATABASE_URL=1 cargo test -p comettrace-storage
```

Running the front end against the stack, without rebuilding the image, is in
[deploy/README.md](deploy/README.md#changing-the-front-end-without-rebuilding-anything).

## Releases

A tag `vX.Y.Z` publishes two images for linux/amd64 and linux/arm64:
`ghcr.io/comettrace/comettrace:X.Y.Z` is the community build and
`:X.Y.Z-ee` the cloud one. Both carry provenance and an SBOM.

## License

Apache-2.0. See [LICENSE](LICENSE).
