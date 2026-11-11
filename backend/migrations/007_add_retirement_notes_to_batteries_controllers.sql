-- Adds retirement_notes to batteries and remote_controllers.
-- Aircraft already has this column from 001_initial_schema.sql.
-- Used to capture the reason for retirement (e.g. beyond repair, sold, end of service life).

ALTER TABLE batteries
    ADD COLUMN IF NOT EXISTS retirement_notes TEXT;

ALTER TABLE remote_controllers
    ADD COLUMN IF NOT EXISTS retirement_notes TEXT;
