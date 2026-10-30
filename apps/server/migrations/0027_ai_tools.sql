-- Each AI tool now has its own setup: off, StarBooru's own models or any
-- OpenAI-compatible API. Ollama isn't used any more: a custom Ollama becomes
-- an API, the one that came with the Compose file becomes the built-in models.
CREATE TABLE ai_tools (
    tool      TEXT PRIMARY KEY,
    mode      TEXT NOT NULL,
    preset    TEXT NOT NULL DEFAULT 'balanced',
    model     TEXT NOT NULL DEFAULT '',
    api_url   TEXT NOT NULL DEFAULT '',
    api_key   TEXT NOT NULL DEFAULT '',
    api_model TEXT NOT NULL DEFAULT ''
);

INSERT INTO ai_tools (tool, mode, api_url, api_key, api_model)
SELECT tool, CASE
        WHEN route = 'hosted' OR (route = 'local' AND own_ollama) THEN 'api'
        WHEN route = 'local' THEN 'local'
        ELSE 'off'
    END,
    CASE
        WHEN route = 'hosted' THEN COALESCE(ai_url, '')
        WHEN route = 'local' AND own_ollama THEN ai_local_url || '/v1'
        ELSE ''
    END,
    CASE WHEN route = 'hosted' THEN COALESCE(ai_key, '') ELSE '' END,
    CASE
        WHEN route = 'hosted' THEN COALESCE(ai_model, '')
        WHEN route = 'local' AND own_ollama THEN ai_local_model
        ELSE ''
    END
FROM (
    SELECT tool, s.*,
        COALESCE(
            CASE WHEN tool = 'tagging' THEN s.ai_tagging ELSE s.ai_translation END,
            CASE
                WHEN COALESCE(s.ai_url, '') <> '' THEN 'hosted'
                WHEN s.ai_local_enabled THEN 'local'
            END
        ) AS route,
        COALESCE(s.ai_local_url, '') NOT IN ('', 'http://ollama:11434', 'http://127.0.0.1:11434')
            AND COALESCE(s.ai_local_model, '') <> '' AS own_ollama
    FROM server_settings s
    CROSS JOIN (VALUES ('tagging'), ('translation'), ('comics')) AS tools (tool)
) AS converted
WHERE route IS NOT NULL;

ALTER TABLE server_settings
    DROP COLUMN ai_url,
    DROP COLUMN ai_key,
    DROP COLUMN ai_model,
    DROP COLUMN ai_local_enabled,
    DROP COLUMN ai_local_url,
    DROP COLUMN ai_local_model,
    DROP COLUMN ai_tagging,
    DROP COLUMN ai_translation,
    -- How fast this machine runs the built-in models.
    ADD COLUMN ai_calibration JSONB;
