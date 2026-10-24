# 0002. A tenant is an organisation, not a project

Status: accepted
Date: 2026-10-24

## Context
The span store keys every row on `tenant_id` (ADR 0001) and carries no project column. The control plane holds both organisations and projects, and an ingest key names both. Quotas and retention hang off a plan, and a plan belongs to an organisation. Something has to say which of the two a `tenant_id` is, because the answer decides what a query can isolate and what a quota counts.

## Decision
`tenant_id` is an organisation id. Projects group ingest keys, dashboards and alerts inside an organisation; they do not partition spans. An ingest key records its project so a future migration can add the dimension, and ingestion ignores it until then.

## Alternatives rejected
- A tenant is a project: a plan would have to be duplicated per project, or a quota would have to sum across projects, and the sort key would lead with a column that no plan refers to.
- Both, with `tenant_id` holding either kind of id: two id spaces in one column, and no way to tell which one a row carries.
- A project column in `spans` now: the key would gain a level before any query needs it, and every row would pay for it.

## Consequences
- Gains: one plan, one quota and one retention per `tenant_id`. The sort key matches what a query filters on.
- Costs: a per-project view must filter on a column the sort key does not carry, once that column exists.
- What becomes harder: moving a project between organisations, which would have to rewrite its spans.
