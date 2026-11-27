ALTER TABLE users
    ADD COLUMN IF NOT EXISTS oidc_access_token_enc BYTEA,
    ADD COLUMN IF NOT EXISTS oidc_refresh_token_enc BYTEA,
    ADD COLUMN IF NOT EXISTS oidc_token_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS oidc_profile_synced_at TIMESTAMPTZ;
