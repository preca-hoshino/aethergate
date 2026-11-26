-- Æthergate initial PostgreSQL + optional TimescaleDB schema
-- Monthly traffic is strictly tracked by natural calendar month (stat_month = 1st of month in [stats].timezone).
-- No billing, no traffic_multiplier, no quota/cycle fields.

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    oidc_subject TEXT UNIQUE NOT NULL,
    email TEXT,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('admin', 'user')),
    panel_status TEXT NOT NULL DEFAULT 'active' CHECK (panel_status IN ('active', 'disabled')),
    proxy_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    proxy_username TEXT UNIQUE NOT NULL,
    proxy_uuid UUID UNIQUE NOT NULL,
    proxy_password TEXT UNIQUE NOT NULL,
    sub_token TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_token_hash TEXT UNIQUE NOT NULL,
    csrf_token TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_user_sessions_expires_at ON user_sessions (expires_at);

CREATE TABLE IF NOT EXISTS oidc_states (
    state TEXT PRIMARY KEY,
    pkce_verifier TEXT NOT NULL,
    nonce TEXT NOT NULL,
    redirect_to TEXT NOT NULL DEFAULT '/',
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS nodes (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    region TEXT NOT NULL DEFAULT 'default',
    status TEXT NOT NULL DEFAULT 'offline' CHECK (status IN ('online', 'offline', 'degraded')),
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    hmac_secret_enc BYTEA NOT NULL,
    tls_profile JSONB NOT NULL,
    config_revision BIGINT NOT NULL DEFAULT 1,
    user_revision BIGINT NOT NULL DEFAULT 1,
    cadence_mode TEXT NOT NULL DEFAULT 'eco' CHECK (cadence_mode IN ('eco', 'realtime')),
    agent_version TEXT,
    singbox_version TEXT,
    cpu_pct REAL NOT NULL DEFAULT 0,
    mem_pct REAL NOT NULL DEFAULT 0,
    mem_used_mib INTEGER NOT NULL DEFAULT 0,
    active_connections INTEGER NOT NULL DEFAULT 0,
    active_users INTEGER NOT NULL DEFAULT 0,
    uptime_secs BIGINT NOT NULL DEFAULT 0,
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS node_inbounds (
    id UUID PRIMARY KEY,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    protocol TEXT NOT NULL CHECK (protocol IN ('vless_reality', 'hysteria2', 'tuic', 'anytls')),
    transport TEXT NOT NULL CHECK (transport IN ('tcp', 'udp')),
    listen TEXT NOT NULL DEFAULT '::',
    listen_port INTEGER NOT NULL CHECK (listen_port BETWEEN 1 AND 65535),
    port_range TEXT,
    protocol_config JSONB NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (node_id, tag)
);
CREATE INDEX IF NOT EXISTS idx_node_inbounds_node_id ON node_inbounds (node_id);

CREATE TABLE IF NOT EXISTS node_groups (
    id UUID PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS node_group_members (
    group_id UUID NOT NULL REFERENCES node_groups(id) ON DELETE CASCADE,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, node_id)
);

CREATE TABLE IF NOT EXISTS user_node_grants (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, node_id)
);

CREATE TABLE IF NOT EXISTS user_node_group_grants (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    group_id UUID NOT NULL REFERENCES node_groups(id) ON DELETE CASCADE,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, group_id)
);

CREATE TABLE IF NOT EXISTS node_user_deltas (
    id BIGSERIAL PRIMARY KEY,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    rev BIGINT NOT NULL,
    op TEXT NOT NULL CHECK (op IN ('upsert', 'revoke')),
    user_id UUID NOT NULL,
    proxy_username TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (node_id, rev)
);
CREATE INDEX IF NOT EXISTS idx_node_user_deltas_node_rev ON node_user_deltas (node_id, rev);

CREATE TABLE IF NOT EXISTS node_commands (
    id UUID PRIMARY KEY,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'acked', 'failed')),
    result_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acked_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_node_commands_node_status ON node_commands (node_id, status);

CREATE TABLE IF NOT EXISTS telemetry_batch_dedup (
    batch_id UUID PRIMARY KEY,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_telemetry_batch_dedup_received_at ON telemetry_batch_dedup (received_at);

CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY,
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs (created_at DESC);

-- 4 Monthly Traffic Tables (reset to 0 naturally when stat_month advances on the 1st)
CREATE TABLE IF NOT EXISTS user_monthly_traffic (
    stat_month DATE NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    uplink_bytes BIGINT NOT NULL DEFAULT 0,
    downlink_bytes BIGINT NOT NULL DEFAULT 0,
    peak_uplink_bps BIGINT NOT NULL DEFAULT 0,
    peak_downlink_bps BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (stat_month, user_id)
);

CREATE TABLE IF NOT EXISTS node_monthly_traffic (
    stat_month DATE NOT NULL,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    uplink_bytes BIGINT NOT NULL DEFAULT 0,
    downlink_bytes BIGINT NOT NULL DEFAULT 0,
    peak_uplink_bps BIGINT NOT NULL DEFAULT 0,
    peak_downlink_bps BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (stat_month, node_id)
);

CREATE TABLE IF NOT EXISTS user_node_monthly_traffic (
    stat_month DATE NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    uplink_bytes BIGINT NOT NULL DEFAULT 0,
    downlink_bytes BIGINT NOT NULL DEFAULT 0,
    peak_uplink_bps BIGINT NOT NULL DEFAULT 0,
    peak_downlink_bps BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (stat_month, user_id, node_id)
);

CREATE TABLE IF NOT EXISTS node_inbound_monthly_traffic (
    stat_month DATE NOT NULL,
    node_id UUID NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    inbound_tag TEXT NOT NULL,
    protocol TEXT NOT NULL,
    uplink_bytes BIGINT NOT NULL DEFAULT 0,
    downlink_bytes BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (stat_month, node_id, inbound_tag)
);

CREATE TABLE IF NOT EXISTS traffic_samples_raw (
    time TIMESTAMPTZ NOT NULL,
    node_id UUID NOT NULL,
    user_id UUID NOT NULL,
    uplink_bytes BIGINT NOT NULL,
    downlink_bytes BIGINT NOT NULL,
    uplink_bps BIGINT NOT NULL,
    downlink_bps BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_traffic_samples_raw_time_node ON traffic_samples_raw (time DESC, node_id);
CREATE INDEX IF NOT EXISTS idx_traffic_samples_raw_time_user ON traffic_samples_raw (time DESC, user_id);

CREATE TABLE IF NOT EXISTS node_inbound_samples_raw (
    time TIMESTAMPTZ NOT NULL,
    node_id UUID NOT NULL,
    inbound_tag TEXT NOT NULL,
    protocol TEXT NOT NULL,
    uplink_bytes BIGINT NOT NULL,
    downlink_bytes BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_node_inbound_samples_raw_time_node ON node_inbound_samples_raw (time DESC, node_id);

-- Optional TimescaleDB hypertable activation when running on TimescaleDB
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb') THEN
        CREATE EXTENSION IF NOT EXISTS timescaledb;
        PERFORM create_hypertable('traffic_samples_raw', 'time', if_not_exists => TRUE);
        PERFORM create_hypertable('node_inbound_samples_raw', 'time', if_not_exists => TRUE);
    END IF;
EXCEPTION
    WHEN OTHERS THEN
        RAISE NOTICE 'Skipping timescaledb hypertable setup: %', SQLERRM;
END $$;
