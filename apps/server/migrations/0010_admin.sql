-- Settings an admin changes in the app. NULL follows the environment variable.
CREATE TABLE server_settings (
    id                 BOOLEAN     PRIMARY KEY DEFAULT true CHECK (id),
    name               TEXT,
    allow_registration BOOLEAN,
    trash_days         INTEGER     CHECK (trash_days >= 0),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO server_settings DEFAULT VALUES;

-- A disabled account can't sign in, and its sessions and keys are refused.
ALTER TABLE users ADD COLUMN disabled_at TIMESTAMPTZ;
-- Most bytes of originals the account may upload; NULL is unlimited.
ALTER TABLE users ADD COLUMN quota_bytes BIGINT CHECK (quota_bytes >= 0);
-- Per-account usage.
CREATE INDEX posts_uploader ON posts (uploader_id);
-- Uploads per day on the stats page.
CREATE INDEX posts_created ON posts (created_at);
