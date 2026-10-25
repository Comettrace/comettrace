-- A connect attempt, kept server side for one use, the way an oidc sign-in attempt already is.
-- The state is what comes back from the provider, and it is the primary key because that is all
-- the callback carries: everything else about the attempt is read from the row rather than
-- trusted from the query string.
--
-- Which organisation and which account are among those things. A callback that could name its
-- own organisation would let anybody who can start a flow attach a credential to somebody
-- else's.
CREATE TABLE integration_attempts (
    state text PRIMARY KEY,
    org_id uuid NOT NULL REFERENCES orgs (id) ON DELETE CASCADE,
    account_id uuid NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind integration_kind NOT NULL,
    created_at bigint NOT NULL,
    expires_at bigint NOT NULL
);

CREATE INDEX integration_attempts_by_expiry ON integration_attempts (expires_at);
