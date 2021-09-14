-- Your SQL goes here
ALTER TABLE signups
	DROP CONSTRAINT signups_user_id_fkey,
	ADD FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
