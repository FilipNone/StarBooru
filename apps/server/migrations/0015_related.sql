-- Related posts: finds live posts that look alike through the four 16-bit
-- chunks of the difference hash (see `similar.rs`), one index per chunk.
CREATE INDEX posts_phash_c0 ON posts ((phash & 65535)) WHERE deleted_at IS NULL;
CREATE INDEX posts_phash_c1 ON posts (((phash >> 16) & 65535)) WHERE deleted_at IS NULL;
CREATE INDEX posts_phash_c2 ON posts (((phash >> 32) & 65535)) WHERE deleted_at IS NULL;
CREATE INDEX posts_phash_c3 ON posts (((phash >> 48) & 65535)) WHERE deleted_at IS NULL;
