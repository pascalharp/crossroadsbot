-- Your SQL goes here
CREATE TABLE signup_board_channels (
	day DATE PRIMARY KEY,
	channel_id BIGINT NOT NULL
);
ALTER TABLE trainings
ADD board_message_id BIGINT DEFAULT NULL;
