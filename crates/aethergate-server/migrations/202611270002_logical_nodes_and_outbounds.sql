-- Support multi-egress physical hosts with child logical nodes, custom outbounds/DNS/route,
-- per-inbound outbound binding, and relay service accounts (to prevent 2x user traffic double-counting).

ALTER TABLE nodes
    ADD COLUMN IF NOT EXISTS parent_node_id UUID REFERENCES nodes(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS outbounds JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS dns_route JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS idx_nodes_parent_node_id ON nodes (parent_node_id);

ALTER TABLE node_inbounds
    ADD COLUMN IF NOT EXISTS outbound_tag TEXT;

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS account_type TEXT NOT NULL DEFAULT 'user'
        CHECK (account_type IN ('user', 'relay'));
