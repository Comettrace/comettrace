# Self-hosting Comettrace

One command brings up the server, its control plane, the span store and the
usage counter:

```sh
cd deploy
docker compose up --build --detach
```

The server waits for all three stores to report healthy, applies the
Postgres and ClickHouse migrations itself, and then listens on 4317 for
OTLP over gRPC and 4318 for everything else: OTLP over HTTP, the API, and
the web application at <http://localhost:4318>.

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
  no preparation. ClickHouse and Redis are reachable only from inside the
  compose network; Postgres answers this machine too, on the loopback, so
  a database client can reach it. Set `POSTGRES_PASSWORD` and
  `CLICKHOUSE_PASSWORD` in a `.env` file beside the compose file for
  anything that is not a laptop — and before widening `POSTGRES_BIND`,
  which is what would put that database on the network.
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
| `POSTGRES_BIND` | `127.0.0.1` | Which address the control plane answers on |
| `POSTGRES_PORT` | `5432` | Host port for the control plane |
| `COMETTRACE_WEB_ROOT` | set in the image | Where the built web application lives |
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

## Adding somebody

Settings > Organization > Members does it: an owner or an administrator
invites an address, and gets back one link that sets that account's
password. The link lasts 24 hours and can be spent once, and it is shown
once — nothing can read it back, because the control plane keeps only its
digest. Send it through whatever channel you already trust.

An address that already has an account here joins without a link: it signs
in with the password it already chose. Nothing on that screen can set a
password for an account somebody else signs in with.

Only an owner hands out the owner role, changes an owner or removes one,
and an organisation keeps at least one owner. So the sole owner of a fresh
deployment can rename the organisation, invite anybody and change anybody
else, but cannot stop being the owner until it has made somebody else one —
after which it can step down, or leave. An organisation with no owner left
could only be rescued with `grant-role`, which is why that is refused.

Somebody who has lost their password asks for a link on the sign-in
screen, which needs the mail relay. Without one, print a link yourself:

```sh
docker compose exec server comettrace reset-password --email somebody@example.com
```

`create-account` and `grant-role` still do the same job from the command
line, which is how you add the first person when nobody can sign in yet.

## Projects

Telemetry arrives under a project, and `bootstrap` makes the first one,
`default`. Settings > Organization > Projects makes the rest, renames them,
hands them to a team and deletes them; `create-project` still does the
first two from the command line.

A name is a slug — lowercase letters, digits and dashes — because it is
typed into a configuration file beside an ingest key and read out of a url.
What you type is lowered rather than refused, so `Payments-EU` is kept as
`payments-eu`.

Renaming keeps the keys and the signals: a key carries the project id and
so does every span, and nothing anywhere holds the name. Deleting is not
the same bargain. The api keys go with the project at once, and the spans
already written stay in the span store until their retention expires —
unreachable from the application, because nothing names them any more.

How long a signal is kept is a property of the deployment rather than of
one project: `spans.retention_days`, which ClickHouse applies on its own
schedule.

## Multi-tenancy

Organisations exist and every span carries one, but a self-hosted
deployment is expected to have exactly one. `bootstrap` creates it and
refuses to create a second. Nothing stops you adding more with
`create-org` and `grant-role`.

## Looking at the databases

Postgres answers `localhost:5432` as `comettrace` / `comettrace`, so any
client reaches it:

```sh
psql postgres://comettrace:comettrace@localhost:5432/comettrace
```

ClickHouse and Redis are not published. Reach them through the container:

```sh
docker compose exec clickhouse clickhouse-client \
  --user=comettrace --password=comettrace --database=comettrace
docker compose exec redis redis-cli
```

## Changing the front end without rebuilding anything

The container serves the built application, so a change to `web/` would
mean a new image. It does not have to: the dev server proxies every server
path to the container, so run it beside the stack instead.

```sh
docker compose up --detach          # the server, on 4318
cd ../web && pnpm install && pnpm dev
```

Then open <http://localhost:5173>. Every change appears without a reload,
and `/auth`, `/api`, `/health` and `/v1` go to the real server on 4318. A
session cookie is set for `localhost` and ignores the port, so signing in
on either one signs you in on both.

`COMETTRACE_SERVER` points the proxy somewhere other than
`http://localhost:4318`.

Rebuild the image when you change Rust, or to check what the single
container actually serves.
