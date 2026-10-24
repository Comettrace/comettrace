# 0012 — A self-hosted stack is one command and one bootstrap

- Status: accepted
- Date: 2026-10-24

## Context

A self-hoster needs four processes and an account that can sign in. The
server now migrates both stores it owns, so the remaining work is bringing
the processes up and making the first account.

"On first start, create a default org" cannot be done by the server alone:
an organisation needs an owner, and only the operator knows which address
that is. Nor can it be a seed migration, because a migration runs in every
deployment including the hosted one.

## Decision

`comettrace bootstrap --email <address>` creates the account if it is
absent, creates one organisation owned by it, and prints a single-use
reset token. **It does nothing when an organisation already exists**, so it
is safe to run again, safe to put in an entrypoint, and cannot quietly
give a deployment a second organisation.

It does not draw an ingest key. Each command prints at most one secret on
stdout, the way `issue-key` and `create-account` already do, so a pipeline
that captures stdout captures exactly one thing. `bootstrap` prints the
`issue-key` line to run next on stderr instead.

It is not run automatically by the compose file. An automatic bootstrap
would have to put the reset token in the service log, where it would sit
for the life of the deployment.

`GET /health` answers 200 with no credentials. It is a liveness answer:
the process is listening. Because migrations run before either listener
starts, a server that answers it has already applied both schemas — which
is what `depends_on: service_healthy` needs to mean.

The compose file carries `comettrace` as the default password for
Postgres and ClickHouse, so `docker compose up` works on a clean machine
with no preparation, which is the acceptance criterion. **No database port
is published to the host**: only 4317 and 4318 are, so those defaults are
reachable only from inside the compose network. The README says to set
them for anything that is not a laptop.

`COOKIE_SECURE` defaults to false because the compose file serves plain
HTTP. A secure cookie over plain HTTP is never sent, so defaulting it to
true would make sign-in silently fail on the one setup this file
describes.

## Consequences

The CI job builds the image, brings the stack up on a clean runner, and
proves: both schemas were applied from inside the container, `bootstrap`
works and is idempotent, the token sets a password, the password opens a
session, `/auth/me` carries the owner role, a short password is refused
with 422, an issued key is accepted and an unissued one is refused with
401.

**It does not prove a span reaches ClickHouse.** It asserts the `spans`
table exists in the configured database, which proves the server reached
ClickHouse with the configured credentials, but the payload it posts is
empty so nothing is written. Proving a round-trip needs an encoded OTLP
fixture, and hand-rolling protobuf in a shell script is a worse thing to
own than this gap.

Images are not published anywhere. The compose file builds from the
checkout, so a self-hoster needs the source. Publishing on semver tags is
#67 and stays there.
