-- Your SQL goes here
CREATE TABLE signup_roles (
	id SERIAL PRIMARY KEY,
	signup_id INTEGER NOT NULL,
	role_id INTEGER NOT NULL,
	FOREIGN KEY(signup_id) REFERENCES signups(id),
	FOREIGN KEY(role_id) REFERENCES roles(id)
)
