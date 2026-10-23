# 0001. Spans live in one shared ClickHouse table keyed by tenant first

Status: accepted
Date: 2026-10-24

## Context
The cloud edition ingests spans from many tenants into one cluster. Three forces shape the schema. Queries are always scoped to one tenant, so a tenant's rows must be contiguous on disk. Ingestion batches rows across tenants (#60), so the write path cannot be a per-tenant target. Retention differs per tenant and changes when a plan changes, so it cannot be a property of the table.

The self-hosted edition made the opposite call for its own reasons: one application container plus Postgres, with ClickHouse named as the exit at scale (Cometrace ADR 0003). This repository is that exit.

## Decision
One `spans` table, `MergeTree`, shared by every tenant. `PARTITION BY toDate(start_time)`. `ORDER BY (tenant_id, service_name, toStartOfHour(start_time), trace_id)`. Attributes in a `Map(String, String)` column, with `service_name` and `http_status_code` promoted to typed columns. Identifiers as `FixedString(16)` and `FixedString(8)`. Retention through a per-row `expire_at DateTime('UTC')` column and `TTL expire_at DELETE`. Schema changes go through numbered up and down SQL files applied by a migrator that records versions in a `schema_migrations` table.

## Alternatives rejected
- A table or a database per tenant: metadata, merges and background work multiply by tenant count, and a cross-tenant batch insert becomes impossible, which is what #60 needs.
- `trace_id` as a hex `String`, as the official ClickHouse OpenTelemetry exporter does: doubles the bytes in the sort key for no gain, since the domain already holds fixed 16 and 8 byte arrays.
- Attributes as a JSON `String`: loses indexed key access; `Map` supports `attributes['k']` directly.
- Table-level TTL per retention class: a tenant changing plan would have to move rows between tables. A row carries its own expiry instead.
- Retention by dropping whole partitions, as the Postgres store does: partitions are days shared by every tenant, so a drop would cut every tenant at once.

## Consequences
- Gains: one insert target for every tenant. A tenant's scan reads contiguous granules. Retention is a column, so a plan change is an `ALTER TABLE ... UPDATE expire_at`, not a data move.
- Costs: `toStartOfHour(start_time)` in the sort key means a query filtering on a raw timestamp range must let ClickHouse derive the hour bound. The TTL merge rewrites parts in the background.
- What becomes harder: isolating one tenant's storage quota or moving one tenant to its own cluster. Both would need the table split that this decision rejects.
