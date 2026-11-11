-- 015_user_management.sql
-- Adds the deactivation timestamp on users plus the user_tokens table that
-- backs invitation flows and admin-issued password resets. The same table
-- serves both via the `purpose` discriminator; only `consumed_at IS NULL`
-- and `expires_at > NOW()` rows are redeemable. Stored as bcrypt(token)
-- so a DB read can never recover plaintext tokens.

ALTER TABLE users ADD COLUMN IF NOT EXISTS deactivated_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS last_login_at  TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS user_tokens (
    id            BIGSERIAL PRIMARY KEY,
    org_id        INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose       TEXT NOT NULL CHECK (purpose IN ('invitation', 'password_reset')),
    token_hash    TEXT NOT NULL,
    issued_by     INTEGER REFERENCES users(id),
    expires_at    TIMESTAMPTZ NOT NULL,
    consumed_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_user_tokens_org_user
  ON user_tokens(org_id, user_id);
CREATE INDEX IF NOT EXISTS idx_user_tokens_unconsumed
  ON user_tokens(user_id) WHERE consumed_at IS NULL;
