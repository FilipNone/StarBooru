-- Near-duplicate finder: a 64-bit difference hash and the average colour
-- (0xRRGGBB) of each thumbnail, and the pairs people marked as "not
-- duplicates".
ALTER TABLE posts ADD COLUMN phash BIGINT, ADD COLUMN pcolor INTEGER;

CREATE TABLE duplicate_dismissals (
    post_a     BIGINT NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    post_b     BIGINT NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (post_a, post_b),
    CHECK (post_a < post_b)
);
CREATE INDEX duplicate_dismissals_b ON duplicate_dismissals (post_b);
