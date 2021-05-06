-- Your SQL goes here
CREATE TABLE roles (
	id SERIAL PRIMARY KEY,
	title TEXT NOT NULL,
	repr TEXT NOT NULL,
	emoji BIGINT NOT NULL,
	active BOOL NOT NULL DEFAULT TRUE
);

CREATE UNIQUE INDEX roles_active_constraint ON roles (repr, emoji) WHERE active
