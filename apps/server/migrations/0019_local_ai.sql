-- Local AI (an Ollama runtime) and where each AI tool runs. NULL follows
-- the environment variable, like the other AI settings.
ALTER TABLE server_settings ADD COLUMN ai_local_enabled BOOLEAN;
ALTER TABLE server_settings ADD COLUMN ai_local_url TEXT;
ALTER TABLE server_settings ADD COLUMN ai_local_model TEXT;
-- 'off', 'hosted' or 'local'.
ALTER TABLE server_settings ADD COLUMN ai_tagging TEXT;
ALTER TABLE server_settings ADD COLUMN ai_translation TEXT;
