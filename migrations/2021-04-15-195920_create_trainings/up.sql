-- Your SQL goes here
CREATE TYPE training_state AS ENUM ('created', 'published', 'closed', 'finished');
CREATE TABLE trainings (
	id SERIAL PRIMARY KEY,
	title TEXT NOT NULL,
	date TIMESTAMP NOT NULL,
	state training_state NOT NULL DEFAULT 'created'
)
