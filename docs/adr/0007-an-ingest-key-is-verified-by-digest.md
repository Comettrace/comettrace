# 0007. An ingest key is verified by digest, superseding 0003

Status: accepted
Date: 2026-10-24

## Context
ADR 0003 decided that an ingest key is found by a unique clear prefix and verified with argon2, and listed "one argon2 verification per request" among its gains. ADR 0006, written later, rejects exactly that shape for session tokens: "every authenticated request would spend 19 MiB and tens of milliseconds to re-derive a digest of a value that was already random".

An ingest key is drawn by this service, not chosen by a person. It has the same shape as a session token and the same threat model: there is no dictionary to try, only the whole space. Argon2 exists to make a guess expensive against a low-entropy secret, and it buys nothing here while costing the hottest path in the gateway tens of milliseconds per payload, on a path measured at 0.11 seconds for 60 000 spans (docs/benchmarks/clickhouse-batching.md).

## Decision
A key stays `<prefix>.<secret>`, and the prefix keeps its `UNIQUE` constraint, so a lookup is still one indexed row. The secret is stored as its SHA-256 in `secret_digest` and compared in constant time. A key whose row carries no digest cannot authenticate.

## Alternatives rejected
- Keep argon2, as ADR 0003 decided: tens of milliseconds and 19 MiB per payload for a secret that was already random, on the path that carries all telemetry traffic.
- Keep argon2 behind a cache of verified keys: the first payload of every key still pays it, and a revoked key keeps working until the cache expires.
- Argon2 at a lower cost: a cost low enough for this path is a cost too low to matter against a dictionary, so it would be argon2 in name only.
- Convert the existing argon2 rows: a key's plaintext is unknown once issued, so a stored PHC string cannot become a digest. Those keys have to be reissued.

## Consequences
- Gains: the key check costs one indexed read and one digest comparison, so authentication no longer bounds ingestion throughput.
- Costs: a stolen database yields digests of high-entropy secrets, which is the same exposure a session already has and the reason ADR 0005 accepted it. The old `hash` column stays until a human confirms no key depends on it, because deleting credentials is not a change this repository makes on its own.
- What becomes harder: nothing for a random key. A key a person chose would now be cheap to attack offline, so this decision also forbids ever letting a person pick one.
