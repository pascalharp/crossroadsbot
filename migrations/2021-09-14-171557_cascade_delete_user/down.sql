-- This file should undo anything in `up.sql`
ALTER TABLE signups
	DROP CONSTRAINT signups_user_id_fkey,
	ADD FOREIGN KEY(user_id) REFERENCES users(id)
