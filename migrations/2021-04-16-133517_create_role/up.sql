-- Your SQL goes here
CREATE TABLE roles (
	id SERIAL PRIMARY KEY,
	title TEXT NOT NULL,
	repr TEXT NOT NULL UNIQUE,
	emoji BIGINT NOT NULL UNIQUE
)
