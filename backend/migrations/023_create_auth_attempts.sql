-- 023_create_auth_attempts.sql
-- Per-username rate-limit counter for credential-handling auth endpoints.
--
-- Used by services::rate_limit::check_and_record. The per-IP rate limiter
-- (tower_governor in main.rs) stays in place; this table backs a second
-- layer keyed on the credential identifier itself, which is what actually
-- protects users on a Docker Desktop host (where Docker Desktop NATs every external
-- client to the bridge gateway IP and the per-IP layer collapses into a
-- single shared bucket — see the internal design notes §16).
--
-- Identifier semantics by scope:
--   login           — lowercase trimmed email
--   redeem_token    — sha256(token) hex, full 64 chars (raw token never lands here)
--   change_password — "user:<id>" string (the JWT-authenticated user)

CREATE TABLE auth_attempts (
    id          BIGSERIAL PRIMARY KEY,
    scope       TEXT        NOT NULL,
    identifier  TEXT        NOT NULL,
    attempt_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Rolling-window query: WHERE scope = ? AND identifier = ? AND attempt_at > NOW() - INTERVAL '15 minutes'
CREATE INDEX idx_auth_attempts_scope_identifier_time
    ON auth_attempts (scope, identifier, attempt_at DESC);

-- Cleanup index for the eventual purge job: DELETE WHERE attempt_at < NOW() - INTERVAL '30 days'
CREATE INDEX idx_auth_attempts_attempt_at
    ON auth_attempts (attempt_at);

COMMENT ON TABLE auth_attempts IS
    'Rate-limit counter for auth endpoints (login, redeem_token, change_password). 10 attempts per 15-minute rolling window per (scope, identifier). Rows older than 30 days should be purged by an external cron.';

-- Rollback: DROP TABLE IF EXISTS auth_attempts;
