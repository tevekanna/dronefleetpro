ALTER TABLE maintenance
    ADD COLUMN controller_id INTEGER REFERENCES remote_controllers(id) ON DELETE SET NULL;

ALTER TABLE maintenance
    ADD CONSTRAINT maintenance_target_exactly_one
    CHECK (num_nonnulls(aircraft_id, controller_id) <= 1);

CREATE INDEX idx_maintenance_controller_id ON maintenance (controller_id);

COMMENT ON COLUMN maintenance.controller_id IS
    'Target of this maintenance event when it applies to a remote controller. At most one of aircraft_id or controller_id is set per row — DB CHECK enforces "not both"; the handler''s validate_exactly_one_target enforces "exactly one" on insert. ON DELETE SET NULL on both FKs means orphaned audit rows have NULL on both, which is allowed.';
