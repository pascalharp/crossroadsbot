-- This file should undo anything in `up.sql`
ALTER TABLE trainings
	DROP COLUMN tier_id;

DROP TABLE tiers;
