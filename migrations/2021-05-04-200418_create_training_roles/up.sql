-- Your SQL goes here
CREATE TABLE training_roles (
	id SERIAL PRIMARY KEY,
	training_id INTEGER NOT NULL,
	role_id INTEGER NOT NULL,
	FOREIGN KEY(training_id) REFERENCES trainings(id),
	FOREIGN KEY(role_id) REFERENCES roles(id)
)
