# 0022. An exception is fingerprinted at ingest

Status: accepted
Date: 2026-10-27

## Context
An issue is a group of exceptions that are the same exception. The grouping
rule is a normalisation: two messages that differ only in a duration, an
order id or a quoted field name describe one bug, and a reader wants to see
it once with a count of four hundred.

The rule is a domain decision, not a query. It has to be a pure function with
a test per rule, because the two ways it fails are both fatal: too timid and
every exception is its own issue, too greedy and `HTTP 500` merges with
`HTTP 404`. So `fingerprint(&Occurrence)` lives in `crates/core`.

That leaves where it runs. The count, the first and last sighting and the
ordering all come from `span_events`, which holds one row per exception and
is partitioned by day. Counting issues means grouping a window of those rows.

## Decision
The fingerprint is computed by the sink and stored as
`span_events.fingerprint`, a `FixedString(16)`, so the list is a
`GROUP BY project_id, fingerprint` over a stored value.

```sql
SELECT project_id, fingerprint, count() AS seen, max(time) AS last_seen
FROM span_events
WHERE tenant_id = ? AND name = 'exception' AND time >= ? AND time < ?
GROUP BY project_id, fingerprint
```

Only `span_events` carries the column. The spans table is untouched: a span
that never threw pays nothing for a feature about the ones that did. An event
that is not an exception carries sixteen zero bytes, which is the one
fingerprint the domain never mints.

What a person decides about an issue is not here. It goes to Postgres, keyed
by the same pair, because spans expire on a TTL and "Inès resolved this" must
not. Two ports, one use case: ADR 0019 settled that shape.

## Alternatives rejected
- Group at read time, in Rust, over the rows of the window. The rule then
  applies to history, and changing it costs nothing. It also means reading
  every exception of a day into the server to count them, with no index to
  help, and the answer is a number the database could have produced. It is
  the right choice only for a deployment small enough not to need the
  feature.
- Normalise in SQL, in a materialised view. The view indexes only the
  exceptions, so it costs nothing on healthy spans, and the rule still
  applies to whatever the view is rebuilt over. But the rule then exists
  twice, in Rust and in SQL, and the domain no longer owns the one thing that
  decides what an issue is. Two spellings of a grouping rule drift, and the
  drift shows up as issues silently splitting.
- A fingerprint over the stack trace instead of the message. It is what a
  reader would expect and it is not available: we hold `exception.stacktrace`
  as an opaque string, with no frames, no in-app boundary and no guarantee
  the sdk sent one at all.
- Group across projects. The same library throwing in two projects is two
  issues, because two different people own them.

## Consequences
- Gains: the list is one aggregate query on an ordered table. The rule is a
  pure function with a test per clause, and the fingerprint is the same in
  any process on any day.
- Costs: changing a normalisation rule regroups nothing already stored. Rows
  written under the old rule keep their old fingerprint, so a rule change
  needs a new column, a new read, and a decision about what to do with the
  decisions keyed to the old prints.
- What becomes harder: adding an input to the fingerprint. Taking error spans
  that carry no exception event into issues, which the spec names as PR2,
  changes what the hash is over, so it is a migration and not an afterthought.
- What becomes easier: everything that wants to count exceptions by group.
  Alerting on a new issue reads `min(time)` per fingerprint, and a per-release
  comparison is a second `GROUP BY` on a column that is already there.
