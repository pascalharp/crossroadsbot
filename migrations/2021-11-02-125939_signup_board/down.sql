-- This file should undo anything in `up.sql`
DROP TABLE signup_board_channels;
ALTER TABLE trainings
DROP COLUMN board_message_id;
