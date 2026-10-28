-- Translated text over images (comic and manga panels): one per post and
-- language, made by a vision model. `blocks` is a list of
-- {x, y, w, h, text, translation} with the box in thousandths of the image.
CREATE TABLE post_translations (
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    lang TEXT NOT NULL,
    blocks JSONB NOT NULL,
    model TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (post_id, lang)
);

-- Language new images with text are translated into; '' is off. NULL
-- follows STARBOORU_AI_TRANSLATE_TO.
ALTER TABLE server_settings ADD COLUMN ai_auto_translate TEXT;
