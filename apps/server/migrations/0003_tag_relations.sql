-- Tag aliases and implications.
--
-- An alias is another name for a tag: tagging or searching with the alias
-- uses the tag instead. An implication `tag_id -> implied_id` adds the
-- implied tag whenever a post gets the tag. Implied tags are stored on the
-- post like any other tag, so searches need no extra work.

CREATE TABLE tag_aliases (
    alias      TEXT        PRIMARY KEY,
    tag_id     BIGINT      NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX tag_aliases_tag ON tag_aliases (tag_id);
CREATE INDEX tag_aliases_prefix ON tag_aliases (alias text_pattern_ops);

CREATE TABLE tag_implications (
    tag_id     BIGINT      NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    implied_id BIGINT      NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tag_id, implied_id),
    CHECK (tag_id <> implied_id)
);
CREATE INDEX tag_implications_implied ON tag_implications (implied_id);
