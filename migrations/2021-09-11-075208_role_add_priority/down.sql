-- This file should undo anything in `up.sql`
ALTER TABLE roles
DROP CONSTRAINT roles_priority_range_check,
DROP COLUMN priority
