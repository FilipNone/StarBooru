-- Accounts made by an OpenID Connect sign-in have no password until one is set.
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;

-- Provider sign-ins linked to accounts.
CREATE TABLE user_identities (
    id           BIGSERIAL   PRIMARY KEY,
    user_id      BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    issuer       TEXT        NOT NULL,
    subject      TEXT        NOT NULL,
    -- Username or email the provider gave, to show.
    label        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    UNIQUE (issuer, subject)
);
CREATE INDEX user_identities_user ON user_identities (user_id);

-- Sign-ins in progress; each lasts a few minutes and is used once.
CREATE TABLE oidc_logins (
    state_hash   BYTEA       PRIMARY KEY,
    handle_hash  BYTEA       NOT NULL,
    nonce        TEXT        NOT NULL,
    verifier     TEXT        NOT NULL,
    return_to    TEXT        NOT NULL,
    -- Set when linking to a signed-in account instead of signing in.
    link_user_id BIGINT      REFERENCES users (id) ON DELETE CASCADE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Set once the provider has answered.
    code_hash    BYTEA       UNIQUE,
    issuer       TEXT,
    subject      TEXT,
    label        TEXT
);
CREATE INDEX oidc_logins_created ON oidc_logins (created_at);
