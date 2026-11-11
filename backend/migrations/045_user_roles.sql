-- 045_user_roles.sql
-- Multi-role users (PR 1 of 3): introduce a normalised user_roles join table
-- so a single user account can hold several RBAC roles at once.
--
-- Background: until now `users.role TEXT NOT NULL` held exactly one role and
-- the JWT carried a single `role` claim. This PR keeps that column in place as
-- a denormalised PRIMARY-role mirror (kept in sync by the handlers) for
-- backwards compatibility with already-issued 8h tokens and the not-yet-updated
-- frontend; a later PR drops it once everything reads `roles`.
--
-- Note: the RBAC role 'pilot' here is DISTINCT from the operational `pilots`
-- table (linked via users.pilot_id). This migration only touches RBAC roles.
--
-- Fully idempotent — safe to re-run.

-- ----------------------------------------------------------------------
-- user_roles
-- ----------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS user_roles (
    org_id      INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role        TEXT NOT NULL,
    granted_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    granted_by  INTEGER REFERENCES users(id),
    PRIMARY KEY (user_id, role),
    CHECK (role IN ('admin', 'pilot', 'technician', 'safety_officer', 'viewer'))
);

-- Belt-and-suspenders org isolation, consistent with every other tenant table.
ALTER TABLE user_roles ENABLE ROW LEVEL SECURITY;

CREATE INDEX IF NOT EXISTS idx_user_roles_org  ON user_roles(org_id);
CREATE INDEX IF NOT EXISTS idx_user_roles_role ON user_roles(role);

-- ----------------------------------------------------------------------
-- Backfill: one row per existing user carrying their current single role.
-- ON CONFLICT keeps re-runs safe.
-- ----------------------------------------------------------------------
INSERT INTO user_roles (org_id, user_id, role, granted_by)
SELECT org_id, id, role, NULL
  FROM users
ON CONFLICT (user_id, role) DO NOTHING;
