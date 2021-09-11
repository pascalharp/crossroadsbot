-- Your SQL goes here
ALTER TABLE roles
ADD priority SMALLINT NOT NULL DEFAULT 2,
ADD CONSTRAINT roles_priority_range_check CHECK (
	priority >= 0 AND priority <= 4
)
