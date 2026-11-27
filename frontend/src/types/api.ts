export type ProtocolType = 'vless_reality' | 'hysteria2' | 'tuic' | 'anytls'

export interface AuthUser {
  id: string
  oidc_subject: string
  email: string | null
  display_name: string
  role: 'admin' | 'user'
  panel_status: 'active' | 'disabled'
  proxy_enabled: boolean
  proxy_username: string
  proxy_uuid: string
  proxy_password: string
  sub_token: string
  csrf_token: string
  created_at: string
}

export interface AuthProvidersResponse {
  oidc_enabled: boolean
  provider_name: string
  login_url: string
}

export interface NodeSummary {
  id: string
  parent_node_id: string | null
  name: string
  host: string
  region: string
  status: 'online' | 'offline' | 'degraded'
  tags: string[]
  config_revision: number
  user_revision: number
  cadence_mode: 'eco' | 'realtime'
  cpu_pct: number
  mem_pct: number
  active_connections: number
  active_users: number
  uptime_secs: number
  last_seen_at: string | null
  inbound_count: number
  month_uplink_bytes: number
  month_downlink_bytes: number
}

export interface TlsProfile {
  mode: string
  server_name: string
  cert_path: string
  key_path: string
}

export interface NodeInboundSpec {
  id: string
  protocol: ProtocolType
  tag: string
  enabled: boolean
  listen: string
  listen_port: number
  port_hopping_range: string | null
  outbound_tag?: string | null
  protocol_config: Record<string, unknown>
  sort_order: number
}

export interface NodeDetail {
  id: string
  parent_node_id: string | null
  name: string
  host: string
  region: string
  status: string
  tags: string[]
  config_revision: number
  user_revision: number
  tls_profile: TlsProfile
  outbounds: Array<Record<string, unknown>>
  dns_route: Record<string, unknown>
  inbounds: NodeInboundSpec[]
}

export interface NodeGroup {
  id: string
  name: string
  description: string
  member_count: number
  created_at: string
}

export interface UserSummary {
  id: string
  display_name: string
  email: string | null
  role: 'admin' | 'user'
  account_type: 'user' | 'relay'
  panel_status: 'active' | 'disabled'
  proxy_enabled: boolean
  proxy_username: string
  granted_node_count: number
  month_uplink_bytes: number
  month_downlink_bytes: number
  created_at: string
}

export interface UserDetail {
  id: string
  display_name: string
  email: string | null
  role: 'admin' | 'user'
  panel_status: 'active' | 'disabled'
  proxy_enabled: boolean
  proxy_username: string
  proxy_uuid: string
  sub_token: string
  node_ids: string[]
  group_ids: string[]
}

export interface AnalyticsOverview {
  stat_month: string
  total_uplink_bytes: number
  total_downlink_bytes: number
  online_nodes: number
  total_nodes: number
  active_proxy_users: number
  pending_approval_users: number
  live_connections: number
}

export interface UserMonthlyTraffic {
  user_id: string
  display_name: string
  proxy_username: string
  uplink_bytes: number
  downlink_bytes: number
  peak_uplink_bps: number
  peak_downlink_bps: number
}

export interface NodeMonthlyTraffic {
  node_id: string
  node_name: string
  region: string
  status: string
  uplink_bytes: number
  downlink_bytes: number
  peak_uplink_bps: number
  peak_downlink_bps: number
}

export interface UserNodeMatrixRow {
  user_id: string
  display_name: string
  proxy_username: string
  node_id: string
  node_name: string
  uplink_bytes: number
  downlink_bytes: number
  peak_uplink_bps: number
  peak_downlink_bps: number
}

export interface ProtocolMonthlyRow {
  protocol: ProtocolType
  uplink_bytes: number
  downlink_bytes: number
}

export interface TimeSeriesBucket {
  bucket: string
  uplink_bytes: number
  downlink_bytes: number
}

export interface AuditLogRow {
  id: string
  actor_user_id: string | null
  actor_name: string | null
  action: string
  target_type: string
  target_id: string | null
  detail: Record<string, unknown>
  created_at: string
}

export interface PortalSummary {
  stat_month: string
  proxy_enabled: boolean
  proxy_username: string
  uplink_bytes: number
  downlink_bytes: number
  peak_uplink_bps: number
  peak_downlink_bps: number
  subscription_url: string
  node_usage: Array<{
    node_id: string
    node_name: string
    region: string
    status: string
    uplink_bytes: number
    downlink_bytes: number
  }>
}

export interface PortalSubscriptionLink {
  node_id: string
  node_name: string
  region: string
  inbound_tag: string
  protocol: ProtocolType
  uri: string
}
