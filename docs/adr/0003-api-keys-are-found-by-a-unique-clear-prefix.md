# 0003. API keys are found by a unique clear prefix and verified with argon2

Status: accepted
Date: 2026-10-24

## Context
Every ingested payload presents an API key, so the lookup sits on the hottest path in the gateway. Argon2 is salted by design: the stored hash of a key cannot be recomputed from the key alone, so the key cannot be the lookup term the way a SHA-256 digest can. The self-hosted store does exactly that, indexing `hash bytea` as a primary key, which argon2 forbids.

## Decision
A key is `<prefix>.<secret>`. The prefix is stored in clear and carries a `UNIQUE` constraint; the secret is stored as an argon2 PHC string. A lookup reads the one row the prefix names and verifies the secret against that row.

## Alternatives rejected
- Scan every live key of every organisation and verify each with argon2: one argon2 verification per stored key per request.
- A non-unique prefix: a lookup returns a candidate set, so the cost of a request grows with the number of keys sharing a prefix, and an attacker chooses that number.
- A deterministic hash as the lookup term, as the self-hosted store does: a stolen database then allows an offline dictionary attack at the speed of the hash.
- A cache in front of a scan: the first request of every key still pays the scan, and a revoked key keeps working until the cache expires.

## Consequences
- Gains: one indexed row read and one argon2 verification per request, whatever the number of keys.
- Costs: the prefix is public, so the set of live prefixes is enumerable by anyone who can guess them; it names a key, it does not authenticate one.
- What becomes harder: rotating a key while keeping its prefix, which the unique constraint forbids until the old row is gone.
