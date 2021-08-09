-- Your SQL goes here
CREATE TABLE signup_roles (
	signup_id INTEGER NOT NULL,
	role_id INTEGER NOT NULL,
	FOREIGN KEY(signup_id) REFERENCES signups(id) ON DELETE CASCADE,
	FOREIGN KEY(role_id) REFERENCES roles(id),
	PRIMARY KEY(signup_id, role_id)
)
