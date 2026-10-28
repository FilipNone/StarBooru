-- Text found in images by OCR (tesseract, run locally). ocr_at NULL: not
-- scanned yet (the startup backfill does it); ocr_text NULL with ocr_at
-- set: scanned, no text found.
ALTER TABLE posts ADD COLUMN ocr_text TEXT;
ALTER TABLE posts ADD COLUMN ocr_at TIMESTAMPTZ;

-- `text:` search on live posts.
CREATE INDEX posts_ocr_text ON posts
    USING GIN (to_tsvector('simple', coalesce(ocr_text, '')))
    WHERE deleted_at IS NULL;

-- Still images the backfill has yet to scan.
CREATE INDEX posts_ocr_pending ON posts (id) WHERE ocr_at IS NULL AND kind = 0;
