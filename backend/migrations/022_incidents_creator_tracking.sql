-- ============================================================================
-- Add creator-tracking to incidents to support RBAC "edit own incident" for
-- pilot/technician roles per the matrix in docs/roles-and-permissions.md.
--
-- Existing rows (E2E fixtures, all NULL after this migration) are intentionally
-- left without a creator — only safety_officer/admin will be able to edit them
-- after the matrix is enforced, which matches the desired blast radius.
-- ============================================================================

ALTER TABLE incidents
    ADD COLUMN created_by_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX idx_incidents_created_by_user_id
    ON incidents(created_by_user_id)
    WHERE created_by_user_id IS NOT NULL;
