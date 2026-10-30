-- The people in a post ("figures"), each with tags sorted into slots.
-- A figure's tags are also on posts.tag_ids, so plain tag searches find them;
-- these tables answer "one figure has all of these" searches and the viewer.

CREATE TABLE figures (
    id       BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    post_id  BIGINT   NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    position SMALLINT NOT NULL,
    -- 0 girl, 1 boy, 2 other
    kind     SMALLINT NOT NULL CHECK (kind BETWEEN 0 AND 2),
    UNIQUE (post_id, position)
);

CREATE TABLE figure_tags (
    figure_id BIGINT   NOT NULL REFERENCES figures (id) ON DELETE CASCADE,
    tag_id    BIGINT   NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    -- 0 character, 1 cosplay, then the slots from head (2) to other (12)
    slot      SMALLINT NOT NULL CHECK (slot BETWEEN 0 AND 12),
    position  SMALLINT NOT NULL,
    PRIMARY KEY (figure_id, tag_id)
);
CREATE INDEX figure_tags_tag ON figure_tags (tag_id);
