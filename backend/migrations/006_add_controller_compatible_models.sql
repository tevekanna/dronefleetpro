-- Adds compatible_models to remote_controllers.
-- Stores a free-text description of which aircraft models this controller supports,
-- e.g. "Mavic 3, Mini 4 Pro" or "Matrice 30 / Matrice 300".
-- Grouped on in the Fleet UI to make multi-model controllers manageable.

ALTER TABLE remote_controllers
    ADD COLUMN IF NOT EXISTS compatible_models TEXT;
