-- An import whose site says it is busy (HTTP 429, 503 or 421) waits and tries
-- again, in place of failing: `attempts` counts the tries so far, and
-- `not_before` is when the next one may start. A worker skips a job until then.
ALTER TABLE import_jobs
    ADD COLUMN attempts SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN not_before TIMESTAMPTZ;
