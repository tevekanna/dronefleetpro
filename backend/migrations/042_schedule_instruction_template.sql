-- Link a maintenance schedule to the inspection instruction template whose
-- checklist applies when the schedule is completed.
-- NULL = no checklist linked (schedule can still be completed without one).
-- ON DELETE SET NULL: deactivating/deleting a template must never break a
-- schedule or block EASA traceability (completions reference checklist
-- items directly, not via this column).
ALTER TABLE maintenance_schedules
    ADD COLUMN IF NOT EXISTS instruction_template_id INTEGER
        REFERENCES maintenance_instruction_templates(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_maint_sched_instruction_template
    ON maintenance_schedules(instruction_template_id);

COMMENT ON COLUMN maintenance_schedules.instruction_template_id IS
    'Optional FK to the inspection instruction template whose checklist applies when this schedule is completed. NULL = no checklist linked.';
