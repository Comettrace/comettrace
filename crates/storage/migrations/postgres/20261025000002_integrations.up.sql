CREATE TYPE integration_kind AS ENUM ('linear', 'github', 'jira');

-- One connection per organisation per provider, and the whole credential sealed as one value.
-- Jira hands back an access token, a refresh token and an expiry; GitHub and Linear hand back
-- less. Sealing the lot under one nonce means the shape inside stays the provider's business and
-- this table does not grow a column every time one of them changes its mind.
--
-- The three plain columns are what a connection card shows and none of them is a secret: which
-- workspace or site it is to, who connected it, and when. "Connected" with no account beside it
-- says nothing worth reading.
CREATE TABLE integrations (
    org_id uuid NOT NULL REFERENCES orgs (id) ON DELETE CASCADE,
    kind integration_kind NOT NULL,
    nonce bytea NOT NULL,
    secret bytea NOT NULL,
    account_label text NOT NULL,
    connected_by uuid REFERENCES accounts (id) ON DELETE SET NULL,
    created_at bigint NOT NULL DEFAULT ((extract(epoch FROM clock_timestamp()) * 1000000000)::bigint),
    PRIMARY KEY (org_id, kind)
);
