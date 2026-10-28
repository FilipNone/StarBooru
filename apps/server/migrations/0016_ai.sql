-- AI provider an admin sets in the app. NULL follows the environment variable.
ALTER TABLE server_settings ADD COLUMN ai_url TEXT;
ALTER TABLE server_settings ADD COLUMN ai_model TEXT;
-- Write-only: never returned to clients.
ALTER TABLE server_settings ADD COLUMN ai_key TEXT;
ALTER TABLE server_settings ADD COLUMN ai_autotag BOOLEAN;
