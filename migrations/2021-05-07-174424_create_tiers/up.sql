-- Your SQL goes here
CREATE TABLE tiers (
	id SERIAL PRIMARY KEY,
	name TEXT UNIQUE NOT NULL
);

ALTER TABLE trainings
	ADD COLUMN tier_id INT REFERENCES tiers(id);
