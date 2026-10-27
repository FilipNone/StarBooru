-- Sessions get an id their owner can refer to, when they were last used, and
-- a short client description ("Firefox on Linux"), for the settings page.
ALTER TABLE sessions ADD COLUMN id BIGINT GENERATED ALWAYS AS IDENTITY;
CREATE UNIQUE INDEX sessions_id ON sessions (id);
ALTER TABLE sessions ADD COLUMN last_used_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE sessions ADD COLUMN client TEXT NOT NULL DEFAULT '';
