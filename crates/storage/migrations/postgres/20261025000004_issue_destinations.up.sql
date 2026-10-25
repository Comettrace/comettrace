-- Where an issue goes. One default per connection, and the projects that differ from it.

-- The organisation's default, beside the connection it belongs to: a destination means nothing
-- without the workspace it is in, and this is the row that names the workspace.
--
-- The name rides along with the id because the screen has to say "Engineering" without calling
-- Linear to render a table. It is a copy, so it drifts if somebody renames the team upstream;
-- what it is not is a second source of truth, because only the id is ever sent back.
ALTER TABLE integrations
    ADD COLUMN destination_id text,
    ADD COLUMN destination_name text,
    ADD CONSTRAINT integrations_name_what_they_aim_at
        CHECK ((destination_id IS NULL) = (destination_name IS NULL));

-- So a destination below can pin the project and its organisation in one constraint.
ALTER TABLE projects ADD CONSTRAINT projects_unique_in_their_org UNIQUE (org_id, id);

-- A project that files somewhere other than the default.
--
-- The two composite keys are what hold it together. One pins the project to the organisation, so
-- no row can aim one organisation's project at another's workspace. The other points at the
-- connection, which means disconnecting a provider drops every project's choice for it — and that
-- is right: a team in a workspace nobody is connected to any more is not somewhere to file.
CREATE TABLE issue_destinations (
    org_id uuid NOT NULL,
    project_id uuid NOT NULL,
    kind integration_kind NOT NULL,
    destination_id text NOT NULL,
    destination_name text NOT NULL,
    created_at bigint NOT NULL DEFAULT ((extract(epoch FROM clock_timestamp()) * 1000000000)::bigint),
    PRIMARY KEY (project_id, kind),
    FOREIGN KEY (org_id, project_id) REFERENCES projects (org_id, id) ON DELETE CASCADE,
    FOREIGN KEY (org_id, kind) REFERENCES integrations (org_id, kind) ON DELETE CASCADE
);

CREATE INDEX issue_destinations_by_org ON issue_destinations (org_id, kind);
