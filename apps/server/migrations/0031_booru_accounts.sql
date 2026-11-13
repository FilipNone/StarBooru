-- Accounts on other boorus, saved so that a key is typed once and no app has
-- to keep it. `sealed_key` is the site's API key encrypted with a key derived
-- from STARBOORU_SECRET (ChaCha20-Poly1305: a 12-byte nonce, then the
-- ciphertext and its tag), bound to the account and the site it belongs to,
-- so a row copied to another user or site doesn't open. The key never leaves
-- the server again: it only goes to that site.
CREATE TABLE booru_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    api TEXT NOT NULL,
    base TEXT NOT NULL,
    login TEXT NOT NULL,
    sealed_key BYTEA NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, base)
);
