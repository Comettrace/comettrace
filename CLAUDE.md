# Comettrace

Self-hosted distributed tracing. OTLP in over gRPC (4317) and HTTP (4318),
spans in ClickHouse, control plane in Postgres, usage counter in Redis, a
React front end served by the same binary.

Start with [README.md](README.md) for the layout and
[deploy/README.md](deploy/README.md) for running the stack.

## Layers

```
crates/api, crates/storage, apps/server  ->  crates/core
```

- `crates/core` is the domain and owns every port trait. It declares no
  infrastructure dependency and must keep declaring none.
- `crates/storage` implements the driven ports: Postgres, ClickHouse, Redis,
  S3, SMTP, OIDC.
- `crates/api` is the driving side: axum router, tonic service, DTOs. A DTO
  never leaves this crate; it becomes a domain type through `TryFrom` at the
  edge.
- `apps/server` is the only place that names a concrete adapter. Ports are
  wired by enum dispatch, not `dyn`. `main.rs` is configuration, wiring and
  the server; `cli.rs` is the clap definitions and the printing, over the
  `Provisioning` service. Neither holds a rule.
- `crates/ee` is the cloud build behind the `ee` feature, and the `ee-gate`
  CI job proves the default build excludes it.

## Conventions

- No comments in code. The reason for a change goes in the commit message,
  or in an ADR when it is a decision.
- Every architecture decision is an ADR in `docs/adr`, numbered, in the shape
  the existing ones use. The code and its ADR must not disagree.
- Commits are conventional and lowercase: `feat(orgs): ...`, `fix(photos):
  ...`, `chore: ...`, with `!` for a breaking change.
- Everything written into the repository is in English.
- Static dispatch first, then enum dispatch, then `dyn` with a reason. No
  `async-trait`.
- Errors are `thiserror` enums per boundary. `anyhow` only in `apps/server`.
- Secrets are newtypes that zeroize and refuse to print themselves.

## Commands

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && pnpm install && pnpm typecheck
```

Integration tests skip silently without a backend unless the matching
`REQUIRE_*` variable is set, which is what CI sets:

```sh
DATABASE_URL=... REQUIRE_DATABASE_URL=1 cargo test -p comettrace-storage
CLICKHOUSE_URL=... REQUIRE_CLICKHOUSE_URL=1 cargo test -p comettrace-storage
REDIS_URL=... REQUIRE_REDIS_URL=1 cargo test -p comettrace-storage
```

The guards live in one place: `crates/storage/tests/backend/mod.rs`.

## Working agreement

The maintainer runs the compile loop. Write the code, say what needs
checking, and leave `cargo build`, `cargo clippy`, `cargo test`,
`docker compose build` and `docker compose up` to him: cargo takes an
exclusive lock on `target/`, and a compose rebuild restarts a running dev
stack.

`tools/` is local scaffolding and is deliberately not committed.
