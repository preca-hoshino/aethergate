-- Stores customizable panel settings such as the Mihomo / Clash Meta subscription DNS & rules template.

CREATE TABLE IF NOT EXISTS panel_settings (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
