# 0016. A retried export may duplicate spans

Status: accepted
Date: 2026-10-26

## Context
`ClickhouseSink::accept` writes one export into two tables: `spans`, then `span_events`. Each table has its own `Inserter`, and each `commit` may or may not flush, depending on whether the batch has crossed `max_rows` or its period.

A failure on the second commit returns `SinkError::Unavailable` for the whole call. `crates/api/src/error.rs` maps that to HTTP 503 and gRPC `unavailable`, both of which the OTLP specification marks retryable, so a conforming exporter resends the identical export. If the first commit had flushed, those spans are written a second time.

`spans` was a `MergeTree` ordered by `(tenant_id, service_name, toStartOfHour(start_time), trace_id)` with no version column and no deduplicating engine, so both copies stayed. A trace then showed each span twice, and a service summary counted each twice. The same hazard exists wherever a 503 is raised after a commit that actually flushed, so splitting the tables is not what creates it.

ADR 0001 chose the schema before this failure mode was considered, and the ordering of the two commits was chosen in `flush` for a different reason: spans without their events read as a trace that recorded none, which is wrong but legible, while events without their spans are rows nothing can reach. That reasoning is about a lost half, not a duplicated one, and it still holds, so the commit order is unchanged.

## Decision
A span replaces itself instead of joining itself.

Migration `0005_replacing_spans` rebuilds both tables as `ReplacingMergeTree`, with the identity of the row at the end of the sorting key:

- `spans`: `ORDER BY (tenant_id, service_name, toStartOfHour(start_time), trace_id, span_id)`
- `span_events`: `ORDER BY (tenant_id, service_name, toStartOfHour(time), name, trace_id, span_id, time)`

Partition keys, TTLs, columns and skip indexes are carried over unchanged. A table's engine and sorting key cannot be altered in place, so the migration creates the replacement, copies the rows with an explicit column list, renames both tables in one statement and drops the old one. Its `down` does the same in reverse, collapsing with `FINAL` on the way out so the plain `MergeTree` it restores holds no duplicate.

The four reads in `crates/storage/src/clickhouse/read.rs` carry `FINAL`, so a reader never waits for a background merge to see one row per span.

`schema_migrations` in this same database was already a `ReplacingMergeTree` read with `FINAL`, so the pattern is not new here.

## Alternatives rejected
- Leave it. A duplicate needs a commit that flushed and a failure in the same export, which is rare. The cost lands on the reader of a trace during an incident, which is the worst moment to show a wrong waterfall.
- Make `accept` report what was durably written and raise a retryable error only when nothing was. `Inserter::commit` returns the quantities it wrote, so this is reachable, but it turns one call into a state machine and the client still has no way to send the remainder only.
- Deduplicate on read with `LIMIT 1 BY (trace_id, span_id)`. No migration, but every read pays for it forever, including the ones that are not affected, and nothing ever removes the duplicate rows.

## Consequences
- Gains: an exporter that retries, for any reason, cannot make a reader see a span twice. Duplicates already written are collapsed by the first merge that touches them, and hidden before it.
- Costs: the upgrade rewrites both span tables once, so a deployment holding a large `spans` table pays a full copy while the migration runs. Every read now merges on the fly. If that becomes the bottleneck, the next knob is `do_not_merge_across_partitions_select_final = 1`, which is sound here because the partition key derives from a column that is part of the sorting key, so two copies of a span always share a partition.
- What becomes harder: two genuinely distinct events of the same span, carrying the same name at the same nanosecond, now collapse into one row. Their attributes are not part of the sorting key and cannot be, so nothing distinguishes them. That shape is pathological and the trade is deliberate.
- The write path is unchanged. `accept` keeps its shape, and the two-inserter split stays as ADR 0001 left it.

## Acceptance
- `a_resent_export_is_read_back_once` in `crates/storage/tests/span_sink.rs`: the same export accepted and flushed twice is counted once through `FINAL`.
- `the_spans_table_carries_the_intended_key_partition_and_ttl` in `crates/storage/tests/clickhouse_migrations.rs` keeps holding: the key still leads with the tenant and still carries the service, the hour and the trace.
- `up_then_down_then_up_leaves_one_recorded_version` keeps holding, which is what proves the rebuild is reversible.
