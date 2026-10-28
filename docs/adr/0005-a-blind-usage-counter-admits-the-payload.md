# 0005. A usage counter that cannot be read admits the payload

Status: accepted
Date: 2026-10-24

## Context
The quota check reads a tenant's month-to-date usage from Redis on every payload. Redis will sometimes be unreachable: a failover, a network partition, an eviction. Something has to decide what the gateway does in that moment, and the decision is not neutral. Refusing admits nothing and loses telemetry a customer cannot re-send, during the very incident they most need traces for. Admitting lets a capped tenant ingest past its plan for the duration of the outage.

## Decision
A read that fails is logged as an error and reported as zero usage, so the payload is admitted. An increment that fails is logged and the payload still proceeds. `UsageReader::used` returns a number rather than a `Result`, which is what makes this the only possible behaviour at the port: the decision lives in the adapter and is recorded here.

## Alternatives rejected
- Refuse while the counter is unreadable: an outage of a counting service becomes an outage of ingestion, and the data is gone for good.
- Fall back to the last known value held in the gateway: every instance would hold a different value, so the cap would depend on which instance answered.
- Read the month-to-date total from Postgres instead: the hourly rollup lags by up to an hour, so the fallback would under-count by design and still need this decision when Postgres is also unreachable.

## Consequences
- Gains: a counter outage costs billing accuracy, never customer telemetry. The request path has one failure mode fewer.
- Costs: a capped tenant may ingest past its plan while the counter is unreadable. The error log is the only record that it happened, so that log is the alert.
- What becomes harder: a hard guarantee that a plan is never exceeded. This design cannot offer one, and `usage_hourly` is where the real total is reconciled.
