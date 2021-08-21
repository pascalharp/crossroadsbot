-- Your SQL goes here
CREATE TABLE tier_mappings (
	tier_id INT NOT NULL,
	discord_role_id BIGINT NOT NULL,
	FOREIGN KEY(tier_id) REFERENCES tiers(id),
	PRIMARY KEY(tier_id, discord_role_id)
);
