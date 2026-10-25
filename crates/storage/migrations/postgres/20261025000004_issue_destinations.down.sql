DROP INDEX issue_destinations_by_org;
DROP TABLE issue_destinations;
ALTER TABLE projects DROP CONSTRAINT projects_unique_in_their_org;
ALTER TABLE integrations DROP CONSTRAINT integrations_name_what_they_aim_at;
ALTER TABLE integrations DROP COLUMN destination_name;
ALTER TABLE integrations DROP COLUMN destination_id;
