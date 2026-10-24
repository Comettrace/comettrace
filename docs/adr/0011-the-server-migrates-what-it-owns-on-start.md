# 0011 — The server migrates what it owns on start

- Status: accepted
- Date: 2026-10-24

## Context

Two things were built, tested and never connected.

`ClickhouseSink` has existed since the spans schema landed, and the
composition root wired `MemorySink` instead — unconditionally. Every
deployment accepted OTLP payloads, answered them, metered them, and held
the spans in a process-local map until it exited. The benchmark that
measured 233× from batching measured a sink nothing called.

Neither migrator ran at start either. `CONTROL_PLANE` and
`ClickhouseMigrator` were exercised only by tests, so a fresh database
carried no tables and every query against it failed. The only way to get
a working instance was to apply migrations by hand.

Together these made `docker compose up` unreachable: it would have brought
up three containers that cannot work with each other.

## Decision

The span store is chosen at start like the meter and the directory are:
a `spans.url` in the configuration selects ClickHouse, its absence selects
the in-memory sink and **says so as a warning**, because silently losing
spans is worse than refusing to start quietly.

The server applies the migrations for both stores it owns, when it is
configured to reach them. The control plane runs `CONTROL_PLANE` after
connecting, and the span store runs `ClickhouseMigrator::up` inside
`open_span_store`, so there is no way to hold a sink whose schema was not
applied.

A batch is committed on the inserter's own deadline rather than on a fixed
tick: a task sleeps `time_left()` and commits. The wait is floored at
100 ms so a zero deadline cannot spin.

The last batch is committed after both listeners have stopped. Without
that, a graceful shutdown drops up to one batch period of spans that the
caller was already told we had accepted.

## Consequences

`spans.retention_days` sets the row TTL and defaults to 30. It is a flat
number for the whole deployment, while `plans.retention_days` is per plan
and unread. Reconciling them is work this does not do, and the per-plan
column stays the one that is wrong to trust today.

Batch size and period are no longer configurable. The benchmark picked
100 000 rows and five seconds, nothing has asked for other numbers, and a
knob with no caller is a knob that drifts out of date.

The control plane migration is safe to run from several instances at once
because sqlx takes an advisory lock. **The ClickHouse migrator takes no
such lock**, so two instances starting against a fresh span store at the
same moment can both try to apply migration 1. Each statement is
`CREATE ... IF NOT EXISTS` and the version table is a `ReplacingMergeTree`,
so the outcome converges, but this has not been tested under a race and it
is the next thing to harden if the deployment ever starts more than one
instance at a time.
