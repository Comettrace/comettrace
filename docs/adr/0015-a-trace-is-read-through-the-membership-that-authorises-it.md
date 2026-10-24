# 0015 — A trace is read through the membership that authorises it

- Status: accepted
- Date: 2026-10-24

## Context

ClickHouse had only a write path. Spans went in and nothing could ask for
one back, so every trace screen in the front end was unbuildable.

This is also the first read in the service that answers for a **person**
rather than for an ingest key. `/v1/traces` authenticates a key that names
its own organisation; `/auth/*` consults no organisation at all. Roles
have existed since `org_members` was read but nothing has ever had to be
authorised.

## Decision

The organisation is a path segment, `/api/orgs/{org}/traces`, and it is
checked against the session's memberships before the span store is
touched. **The authorisation lives in the `Explore` use case, not in the
handler**, so it is tested without a server: a session that holds no
membership of that organisation never reaches the reader at all, and a
test asserts the reader was not asked.

An organisation the session does not belong to answers **404, not 403**.
403 confirms that the organisation exists, which is a fact a stranger has
no business learning. The same answer covers "no such organisation" and
"not yours", and they are indistinguishable from outside.

`/api/` rather than `/v1/`. `/v1/traces` is already OTLP ingestion, with a
different verb, a different body, and a different way of authenticating.
Two meanings of the same path would be a trap for everybody.

**The HTTP layer speaks milliseconds and microseconds, never nanoseconds.**
Unix nanoseconds are around 1.7e18 and JavaScript loses integer precision
above 9.0e15, so a nanosecond timestamp in JSON reaches a browser subtly
wrong. The window is given in milliseconds, a trace reports
`started_at_ms`, and every duration is microseconds. The domain keeps
nanoseconds throughout; only the edge converts.

A span in the trace view carries `start_offset_us` from the earliest span
rather than an absolute time. That is what a waterfall draws, and it keeps
every number small enough to be exact.

Migration 2 adds a bloom filter index on `trace_id`. The sort key is
`(tenant_id, service_name, hour, trace_id)`, so fetching one trace by id
alone would otherwise scan every partition the tenant has.

## Consequences

The service filter is a `HAVING countIf(service_name = ?) > 0`, not a
`WHERE`. A `WHERE` would use the sort key and be much faster, but it would
also filter the rows *before* grouping, so a trace would be summarised
from only the matching service's spans — the wrong span count, the wrong
duration, the wrong root. Correctness won; the cost is a wider scan, and
the fix when it matters is a per-trace rollup table rather than a cleverer
predicate.

The root of a trace is `argMin(service_name, (parent_span_id IS NOT NULL,
start_time))`: the parentless span if the trace has one, else the earliest.
A trace arriving without its root still names something sensible.

A limit is clamped between 1 and 500 in the domain rather than trusted
from the query string.

What this does not do: no free-text attribute search, no dependency graph,
no flame graph, no per-operation metrics, and no logs on a span — there is
no log ingestion at all yet. The services table of #120 is the next piece
and is deliberately not here.
