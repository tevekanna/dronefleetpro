-- Hardening for the audit_log viewer and EASA traceability (4.2.3).
--   1. Indexes for the viewer's filter combinations (org + created_at always required,
--      others optional). Each composite includes created_at DESC so sorted pagination
--      stays on-index.
--   2. Row-level security policy (belt-and-suspenders — primary enforcement remains
--      the application-level "WHERE org_id = $1" in every handler).
--   3. Immutability trigger — audit_log is append-only by regulation.

CREATE INDEX IF NOT EXISTS idx_audit_log_org_created ON audit_log(org_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_org_table   ON audit_log(org_id, table_name, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_org_user    ON audit_log(org_id, user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_org_action  ON audit_log(org_id, action, created_at DESC);

-- RLS was already enabled in 001_initial_schema.sql. Add the org-isolation policy.
DROP POLICY IF EXISTS audit_log_org_isolation ON audit_log;
CREATE POLICY audit_log_org_isolation ON audit_log
    USING (org_id = current_setting('app.current_org_id', TRUE)::INTEGER);

-- Immutability: forbid UPDATE and DELETE on audit_log rows.
CREATE OR REPLACE FUNCTION prevent_audit_log_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'audit_log is immutable — % not allowed', TG_OP;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS audit_log_no_update ON audit_log;
CREATE TRIGGER audit_log_no_update
    BEFORE UPDATE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();

DROP TRIGGER IF EXISTS audit_log_no_delete ON audit_log;
CREATE TRIGGER audit_log_no_delete
    BEFORE DELETE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();
