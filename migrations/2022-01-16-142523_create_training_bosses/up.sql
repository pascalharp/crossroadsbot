-- Your SQL goes here
CREATE TABLE training_bosses (
	id SERIAL PRIMARY KEY,
	repr TEXT UNIQUE NOT NULL,
	name TEXT NOT NULL,
	wing INT NOT NULL,
	position INT NOT NULL,
	UNIQUE (wing, position)
);
CREATE TABLE training_boss_mappings (
	training_id INT NOT NULL,
	training_boss_id INT NOT NULL,
	FOREIGN KEY(training_id) REFERENCES trainings(id) ON DELETE CASCADE,
	FOREIGN KEY(training_boss_id) REFERENCES training_bosses(id) ON DELETE CASCADE,
	PRIMARY KEY(training_id, training_boss_id)
);
CREATE TABLE signup_boss_preference_mappings (
	signup_id INT NOT NULL,
	training_boss_id INT NOT NULL,
	FOREIGN KEY(signup_id) REFERENCES signups(id) ON DELETE CASCADE,
	FOREIGN KEY(training_boss_id) REFERENCES training_bosses(id) ON DELETE CASCADE,
	PRIMARY KEY(signup_id, training_boss_id)
);
