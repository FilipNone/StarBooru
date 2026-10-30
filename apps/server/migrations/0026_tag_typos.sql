-- Finds tags spelled like an unknown one, to correct typos. pg_trgm is a
-- trusted extension (PostgreSQL 13+), so the database's owner can add it.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX tags_name_trgm ON tags USING GIN (name gin_trgm_ops);
