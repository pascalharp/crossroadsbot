-- Your SQL goes here
CREATE TABLE signups (
	id SERIAL PRIMARY KEY,
	user_id INTEGER NOT NULL,
	training_id INTEGER NOT NULL,
	FOREIGN KEY(training_id) REFERENCES trainings(id),
	FOREIGN KEY(user_id) REFERENCES users(id)
)
