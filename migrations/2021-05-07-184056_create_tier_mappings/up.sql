-- Your SQL goes here
CREATE TABLE tier_mappings (
	id SERIAL PRIMARY KEY,
	tier_id INT NOT NULL,
	discord_role_id BIGINT NOT NULL,
	FOREIGN KEY(tier_id) REFERENCES tiers(id)
);
