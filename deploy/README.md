# Self-hosting Comettrace

One command brings up the server, its control plane, the span store and the
usage counter:

```sh
cd deploy
docker compose up --build --detach
```

The server waits for all three stores to report healthy, applies the
Postgres and ClickHouse migrations itself, and then listens on 4317 for
OTLP over gRPC and 4318 for OTLP over HTTP.

## Make it usable

A fresh deployment holds no account and no organisation, so nothing can
sign in and no key can authenticate a payload. One command fixes all of
that:

```sh
docker compose exec server comettrace bootstrap --email you@example.com
```

It creates the account, creates an organisation owned by it, and prints a
single-use token on stdout. The account has no password until you spend
that token:

```sh
curl -X POST http://localhost:4318/auth/reset/confirm \
  -H 'x-requested-with: cli' \
  -H 'content-type: application/json' \
  -d '{"token":"<the printed token>","password":"<at least twelve characters>"}'
```

`bootstrap` does nothing if an organisation already exists, so it is safe
to run again.

## Send a trace

Draw an ingest key for the organisation `bootstrap` named:

```sh
docker compose exec server comettrace issue-key --org <the printed org id>
```

Then point any OTLP exporter at `http://localhost:4318/v1/traces` or
`localhost:4317`, with the key as a bearer token:

```sh
curl -X POST http://localhost:4318/v1/traces \
  -H "authorization: Bearer <the printed key>" \
  -H 'content-type: application/x-protobuf' \
  --data-binary @payload.pb
```

## Using a published image instead of building

Images land on `ghcr.io/comettrace/comettrace` for every released tag:
`1.2.3` is the community build and `1.2.3-ee` the cloud one, both for
linux/amd64 and linux/arm64.

```sh
export COMETTRACE_IMAGE=ghcr.io/comettrace/comettrace:1.2.3
docker compose up --detach --no-build --pull always
```

Without `COMETTRACE_IMAGE` the compose file builds from this checkout,
which is what `--build` above does.

## What to change before this faces anything real

- **The passwords.** Postgres, ClickHouse and the server all default to
  `comettrace` so that `docker compose up` works on a clean machine with
  no preparation. No database port is published to the host, so these are
  reachable only from inside the compose network — but set
  `POSTGRES_PASSWORD` and `CLICKHOUSE_PASSWORD` in a `.env` file beside
  the compose file for anything that is not a laptop.
- **`COOKIE_SECURE`.** It defaults to `false` because this compose file
  serves plain HTTP. Put the server behind TLS and set it to `true`, or
  session cookies travel in the clear.
- **`RETENTION_DAYS`.** Spans carry a TTL of 30 days by default. ClickHouse
  drops them on its own schedule once past it.

## Settings

Every field of `comettrace.toml` can be given as an environment variable:
uppercase, prefixed with `COMETTRACE_`, with `__` between a section and
its field. `COMETTRACE_SPANS__RETENTION_DAYS` is `spans.retention_days`.

| Variable | Default | What it does |
| --- | --- | --- |
| `POSTGRES_PASSWORD` | `comettrace` | The control plane password |
| `CLICKHOUSE_PASSWORD` | `comettrace` | The span store password |
| `RETENTION_DAYS` | `30` | How long a span is kept |
| `COOKIE_SECURE` | `false` | Whether a session cookie demands HTTPS |
| `OTLP_GRPC_PORT` | `4317` | Host port for OTLP over gRPC |
| `OTLP_HTTP_PORT` | `4318` | Host port for OTLP over HTTP |
| `RUST_LOG` | `info` | Log filter |
| `COMETTRACE_IMAGE` | built locally | A published image to run instead |

## Signing in with an identity provider instead

Set the OIDC block and local passwords become unnecessary:

```sh
COMETTRACE_OIDC__ISSUER=https://id.example.com
COMETTRACE_OIDC__CLIENT_ID=comettrace
COMETTRACE_OIDC__CLIENT_SECRET=...
COMETTRACE_OIDC__REDIRECT_URI=https://comettrace.example.com/auth/oidc/callback
COMETTRACE_LOCAL_PASSWORD=false
```

An account is provisioned the first time somebody signs in, so run
`bootstrap` with that person's address afterwards to give them an
organisation to own. It prints no reset token in that case: there is no
password to set, and `/auth/reset/*` is not mounted when local passwords
are off.

## Multi-tenancy

Organisations exist and every span carries one, but a self-hosted
deployment is expected to have exactly one. `bootstrap` creates it and
refuses to create a second. Nothing stops you adding more with
`create-org` and `grant-role`.
