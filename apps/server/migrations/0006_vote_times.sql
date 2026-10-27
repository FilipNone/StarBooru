-- When each vote was cast or last changed, for the explore page's trending list.
-- Votes from before this migration count as cast now.
ALTER TABLE post_votes ADD COLUMN voted_at TIMESTAMPTZ NOT NULL DEFAULT now();
CREATE INDEX post_votes_recent ON post_votes (voted_at);
