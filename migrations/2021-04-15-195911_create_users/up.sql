-- Your SQL goes here
CREATE TABLE users (
	id SERIAL PRIMARY KEY,
	discord_id TEXT NOT NULL UNIQUE,
	gw2_id TEXT NOT NULL UNIQUE
)
