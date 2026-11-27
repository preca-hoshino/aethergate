import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useParams } from '@tanstack/react-router'
import { KeyRound, Plus, RefreshCw, Save, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import type { NodeDetail, NodeInboundSpec, ProtocolType, TlsProfile } from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'

function defaultInboundFor(protocol: ProtocolType, host: string): NodeInboundSpec {
  const id = crypto.randomUUID()
  switch (protocol) {
    case 'vless_reality':
      return {
        id,
        protocol,
        tag: 'in-vless-443',
        enabled: true,
        listen: '::',
        listen_port: 443,
        port_hopping_range: null,
        sort_order: 0,
        protocol_config: {
          type: 'vless_reality',
          server_name: 'www.microsoft.com',
          handshake_server: 'www.microsoft.com',
          handshake_port: 443,
          private_key: '',
          public_key: '',
          short_ids: ['0123456789abcdef'],
          flow: 'xtls-rprx-vision',
          fingerprint: 'chrome',
        },
      }
    case 'hysteria2':
      return {
        id,
        protocol,
        tag: 'in-hy2-443',
        enabled: true,
        listen: '::',
        listen_port: 443,
        port_hopping_range: null,
        sort_order: 1,
        protocol_config: {
          type: 'hysteria2',
          up_mbps: 200,
          down_mbps: 500,
          ignore_client_bandwidth: false,
          obfs: null,
          masquerade: `https://${host}`,
          alpn: ['h3'],
        },
      }
    case 'tuic':
      return {
        id,
        protocol,
        tag: 'in-tuic-8443',
        enabled: true,
        listen: '::',
        listen_port: 8443,
        port_hopping_range: null,
        sort_order: 2,
        protocol_config: {
          type: 'tuic',
          congestion_control: 'bbr',
          udp_relay_mode: 'native',
          zero_rtt_handshake: false,
          auth_timeout: '3s',
          heartbeat: '10s',
          alpn: ['h3'],
        },
      }
    case 'anytls':
      return {
        id,
        protocol,
        tag: 'in-anytls-9443',
        enabled: true,
        listen: '::',
        listen_port: 9443,
        port_hopping_range: null,
        sort_order: 3,
        protocol_config: {
          type: 'anytls',
          padding_scheme: [],
        },
      }
  }
}

interface DnsRouteForm {
  dns_server: string
  dns_server_name: string
  dns_strategy: string
  hijack_dns: boolean
  auto_detect_interface: boolean
  reject_private_ip: boolean
}

interface CustomOutboundItem {
  type: 'direct' | 'hysteria2'
  tag: string
  bind_interface?: string
  routing_mark?: number
  server?: string
  server_port?: number
  password?: string
  server_name?: string
  insecure?: boolean
  alpn?: string[]
}

function parseDnsRoute(raw: Record<string, unknown>): DnsRouteForm {
  return {
    dns_server: typeof raw.dns_server === 'string' ? raw.dns_server : '',
    dns_server_name: typeof raw.dns_server_name === 'string' ? raw.dns_server_name : '',
    dns_strategy: typeof raw.dns_strategy === 'string' ? raw.dns_strategy : '',
    hijack_dns: typeof raw.hijack_dns === 'boolean' ? raw.hijack_dns : true,
    auto_detect_interface:
      typeof raw.auto_detect_interface === 'boolean' ? raw.auto_detect_interface : true,
    reject_private_ip: typeof raw.reject_private_ip === 'boolean' ? raw.reject_private_ip : true,
  }
}

function parseOutbounds(raw: Array<Record<string, unknown>>): CustomOutboundItem[] {
  return raw.map((ob) => ({
    type: ob.type === 'hysteria2' ? 'hysteria2' : 'direct',
    tag: typeof ob.tag === 'string' ? ob.tag : '',
    bind_interface: typeof ob.bind_interface === 'string' ? ob.bind_interface : undefined,
    routing_mark: typeof ob.routing_mark === 'number' ? ob.routing_mark : undefined,
    server: typeof ob.server === 'string' ? ob.server : undefined,
    server_port: typeof ob.server_port === 'number' ? ob.server_port : undefined,
    password: typeof ob.password === 'string' ? ob.password : undefined,
    server_name: typeof ob.server_name === 'string' ? ob.server_name : undefined,
    insecure: typeof ob.insecure === 'boolean' ? ob.insecure : false,
    alpn: Array.isArray(ob.alpn)
      ? ob.alpn.filter((v): v is string => typeof v === 'string')
      : undefined,
  }))
}

export function NodeDetailPage() {
  const { nodeId } = useParams({ from: '/nodes/$nodeId' })
  const queryClient = useQueryClient()

  const { data: node } = useQuery({
    queryKey: ['node-detail', nodeId],
    queryFn: () => api.get<NodeDetail>(`/api/v1/nodes/${nodeId}`),
  })

  const [inbounds, setInbounds] = useState<NodeInboundSpec[]>([])
  const [tls, setTls] = useState<TlsProfile>({
    mode: 'self_signed',
    server_name: '',
    cert_path: '/etc/aethergate/certs/fullchain.pem',
    key_path: '/etc/aethergate/certs/privkey.pem',
  })
  const [dnsRoute, setDnsRoute] = useState<DnsRouteForm>({
    dns_server: '',
    dns_server_name: '',
    dns_strategy: '',
    hijack_dns: true,
    auto_detect_interface: true,
    reject_private_ip: true,
  })
  const [outbounds, setOutbounds] = useState<CustomOutboundItem[]>([])
  const [addProto, setAddProto] = useState<ProtocolType>('vless_reality')

  useEffect(() => {
    if (node) {
      setInbounds(node.inbounds)
      setTls(node.tls_profile)
      setDnsRoute(parseDnsRoute(node.dns_route))
      setOutbounds(parseOutbounds(node.outbounds))
    }
  }, [node])

  const saveInboundsMutation = useMutation({
    mutationFn: () => api.put<{ config_revision: number }>(`/api/v1/nodes/${nodeId}/inbounds`, { inbounds }),
    onSuccess: (res) => {
      void queryClient.invalidateQueries({ queryKey: ['node-detail', nodeId] })
      toast.success(`Saved inbounds (config rev ${res.config_revision})`)
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const saveTlsMutation = useMutation({
    mutationFn: () => api.put<{ config_revision: number }>(`/api/v1/nodes/${nodeId}/tls-profile`, tls),
    onSuccess: (res) => {
      void queryClient.invalidateQueries({ queryKey: ['node-detail', nodeId] })
      toast.success(`Saved TLS profile (config rev ${res.config_revision})`)
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const saveRoutingMutation = useMutation({
    mutationFn: () => {
      const nextDns: Record<string, unknown> = {
        hijack_dns: dnsRoute.hijack_dns,
        auto_detect_interface: dnsRoute.auto_detect_interface,
        reject_private_ip: dnsRoute.reject_private_ip,
      }
      if (dnsRoute.dns_server.trim()) {
        nextDns.dns_server = dnsRoute.dns_server.trim()
      }
      if (dnsRoute.dns_server_name.trim()) {
        nextDns.dns_server_name = dnsRoute.dns_server_name.trim()
      }
      if (dnsRoute.dns_strategy.trim()) {
        nextDns.dns_strategy = dnsRoute.dns_strategy.trim()
      }

      const nextOutbounds: Array<Record<string, unknown>> = outbounds
        .filter((ob) => ob.tag.trim().length > 0)
        .map((ob) => {
          if (ob.type === 'hysteria2') {
            return {
              type: 'hysteria2',
              tag: ob.tag.trim(),
              server: ob.server?.trim() ?? '',
              server_port: ob.server_port ?? 40001,
              password: ob.password ?? '',
              ...(ob.server_name?.trim() ? { server_name: ob.server_name.trim() } : {}),
              insecure: ob.insecure ?? false,
              alpn: ob.alpn?.length ? ob.alpn : ['h3'],
              ...(ob.bind_interface?.trim() ? { bind_interface: ob.bind_interface.trim() } : {}),
              ...(ob.routing_mark !== undefined ? { routing_mark: ob.routing_mark } : {}),
            }
          }
          return {
            type: 'direct',
            tag: ob.tag.trim(),
            ...(ob.bind_interface?.trim() ? { bind_interface: ob.bind_interface.trim() } : {}),
            ...(ob.routing_mark !== undefined ? { routing_mark: ob.routing_mark } : {}),
          }
        })

      return api.patch(`/api/v1/nodes/${nodeId}`, {
        dns_route: nextDns,
        outbounds: nextOutbounds,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['node-detail', nodeId] })
      toast.success('Saved DNS, routing & custom outbounds (SSE pushed)')
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const reloadMutation = useMutation({
    mutationFn: () => api.post(`/api/v1/nodes/${nodeId}/commands`, { action: 'reload_singbox' }),
    onSuccess: () => {
      toast.success('Reload command dispatched via SSE')
    },
  })

  const genRealityKeypair = async (idx: number) => {
    try {
      const pair = await api.post<{ private_key: string; public_key: string }>('/api/v1/nodes/reality-keypair')
      setInbounds((prev) =>
        prev.map((item, i) =>
          i === idx
            ? {
                ...item,
                protocol_config: {
                  ...item.protocol_config,
                  private_key: pair.private_key,
                  public_key: pair.public_key,
                },
              }
            : item,
        ),
      )
      toast.success('Generated X25519 Reality keypair')
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to generate keypair')
    }
  }

  if (!node) {
    return <div className="text-muted-foreground p-6 text-sm">Loading node configuration...</div>
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-2xl font-bold tracking-tight">{node.name}</h1>
            <Badge variant={node.status === 'online' ? 'success' : 'secondary'}>{node.status}</Badge>
          </div>
          <p className="text-muted-foreground font-mono text-xs">
            {node.host} · cfg rev {node.config_revision} · user rev {node.user_revision}
          </p>
        </div>

        <Button
          variant="outline"
          onClick={() => {
            reloadMutation.mutate()
          }}
        >
          <RefreshCw className="size-4" />
          Reload sing-box
        </Button>
      </div>

      {/* Shared Node TLS Profile */}
      <Card>
        <CardHeader>
          <CardTitle>Shared Node TLS Profile</CardTitle>
          <CardDescription>Reused across Hysteria 2, TUIC v5, and AnyTLS inbounds on this node.</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>Certificate Mode</Label>
            <Select value={tls.mode} onValueChange={(v) => { setTls({ ...tls, mode: v }); }}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="self_signed">self_signed (auto-generated by agent)</SelectItem>
                <SelectItem value="external_files">external_files (PEM paths)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <Label>Server Name (SNI)</Label>
            <Input value={tls.server_name} onChange={(e) => { setTls({ ...tls, server_name: e.target.value }); }} />
          </div>
          <div className="space-y-1.5">
            <Label>Cert Path</Label>
            <Input value={tls.cert_path} onChange={(e) => { setTls({ ...tls, cert_path: e.target.value }); }} />
          </div>
          <div className="space-y-1.5">
            <Label>Key Path</Label>
            <Input value={tls.key_path} onChange={(e) => { setTls({ ...tls, key_path: e.target.value }); }} />
          </div>
          <div className="sm:col-span-2">
            <Button
              size="sm"
              disabled={saveTlsMutation.isPending}
              onClick={() => {
                saveTlsMutation.mutate()
              }}
            >
              <Save className="size-4" />
              Save TLS Profile
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* 4-Protocol Inbound Builder */}
      <Card>
        <CardHeader className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <CardTitle>4-Protocol Inbounds</CardTitle>
            <CardDescription>
              TCP and UDP can share ports (e.g. VLESS Reality TCP/443 + Hysteria 2 UDP/443). Same-transport overlaps are blocked.
            </CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <Select value={addProto} onValueChange={(v) => { setAddProto(v as ProtocolType); }}>
              <SelectTrigger className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="vless_reality">VLESS Reality</SelectItem>
                <SelectItem value="hysteria2">Hysteria 2</SelectItem>
                <SelectItem value="tuic">TUIC v5</SelectItem>
                <SelectItem value="anytls">AnyTLS</SelectItem>
              </SelectContent>
            </Select>
            <Button
              variant="outline"
              size="sm"
              onClick={() => { setInbounds((prev) => [...prev, defaultInboundFor(addProto, node.host)]); }}
            >
              <Plus className="size-4" />
              Add
            </Button>
          </div>
        </CardHeader>

        <CardContent className="space-y-4">
          {inbounds.map((ib, idx) => (
            <div key={ib.id || `${ib.tag}-${idx}`} className="space-y-3 rounded-lg border p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <Badge>{ib.protocol}</Badge>
                  <Badge variant="outline">
                    {ib.protocol === 'vless_reality' || ib.protocol === 'anytls' ? 'TCP' : 'UDP'}
                  </Badge>
                </div>
                <div className="flex items-center gap-3">
                  <div className="flex items-center gap-1.5 text-xs">
                    <span>Enabled</span>
                    <Switch
                      checked={ib.enabled}
                      onCheckedChange={(checked) => { setInbounds((prev) => prev.map((x, i) => (i === idx ? { ...x, enabled: checked } : x))); }
                      }
                    />
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => { setInbounds((prev) => prev.filter((_, i) => i !== idx)); }}
                  >
                    <Trash2 className="text-destructive size-4" />
                  </Button>
                </div>
              </div>

              <div className="grid gap-3 sm:grid-cols-4">
                <div className="space-y-1">
                  <Label className="text-xs">Tag</Label>
                  <Input
                    value={ib.tag}
                    onChange={(e) => { setInbounds((prev) => prev.map((x, i) => (i === idx ? { ...x, tag: e.target.value } : x))); }
                    }
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">Listen Port</Label>
                  <Input
                    type="number"
                    value={ib.listen_port}
                    onChange={(e) => { setInbounds((prev) =>
                        prev.map((x, i) => (i === idx ? { ...x, listen_port: Number(e.target.value) } : x)),
                      ); }
                    }
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">Port Hopping Range (Hy2)</Label>
                  <Input
                    placeholder="20000-30000"
                    value={ib.port_hopping_range ?? ''}
                    onChange={(e) => { setInbounds((prev) =>
                        prev.map((x, i) =>
                          i === idx ? { ...x, port_hopping_range: e.target.value || null } : x,
                        ),
                      ); }
                    }
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">Outbound Tag (optional)</Label>
                  <Input
                    placeholder="direct / direct-campus / jp-hy2"
                    value={ib.outbound_tag ?? ''}
                    onChange={(e) => { setInbounds((prev) =>
                        prev.map((x, i) =>
                          i === idx ? { ...x, outbound_tag: e.target.value || null } : x,
                        ),
                      ); }
                    }
                  />
                </div>
              </div>

              {ib.protocol === 'vless_reality' && (
                <div className="grid gap-3 sm:grid-cols-3">
                  <div className="space-y-1">
                    <Label className="text-xs">Reality SNI</Label>
                    <Input
                      value={
                        typeof ib.protocol_config.server_name === 'string'
                          ? ib.protocol_config.server_name
                          : ''
                      }
                      onChange={(e) => {
                        setInbounds((prev) =>
                          prev.map((x, i) =>
                            i === idx
                              ? {
                                  ...x,
                                  protocol_config: {
                                    ...x.protocol_config,
                                    server_name: e.target.value,
                                    handshake_server: e.target.value,
                                  },
                                }
                              : x,
                          ),
                        )
                      }}
                    />
                  </div>
                  <div className="space-y-1">
                    <Label className="text-xs">Private Key</Label>
                    <Input
                      value={
                        typeof ib.protocol_config.private_key === 'string'
                          ? ib.protocol_config.private_key
                          : ''
                      }
                      onChange={(e) => {
                        setInbounds((prev) =>
                          prev.map((x, i) =>
                            i === idx
                              ? {
                                  ...x,
                                  protocol_config: { ...x.protocol_config, private_key: e.target.value },
                                }
                              : x,
                          ),
                        )
                      }}
                    />
                  </div>
                  <div className="flex items-end gap-2">
                    <div className="flex-1 space-y-1">
                      <Label className="text-xs">Public Key</Label>
                      <Input
                        value={
                          typeof ib.protocol_config.public_key === 'string'
                            ? ib.protocol_config.public_key
                            : ''
                        }
                        onChange={(e) => {
                          setInbounds((prev) =>
                            prev.map((x, i) =>
                              i === idx
                                ? {
                                    ...x,
                                    protocol_config: { ...x.protocol_config, public_key: e.target.value },
                                  }
                                : x,
                            ),
                          )
                        }}
                      />
                    </div>
                    <Button
                      variant="outline"
                      size="icon"
                      title="Generate X25519 Keypair"
                      onClick={() => {
                        void genRealityKeypair(idx)
                      }}
                    >
                      <KeyRound className="size-4" />
                    </Button>
                  </div>
                </div>
              )}
            </div>
          ))}

          <Button
            disabled={saveInboundsMutation.isPending}
            onClick={() => {
              saveInboundsMutation.mutate()
            }}
          >
            <Save className="size-4" />
            Save &amp; Push Inbounds
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>DNS, Routing &amp; Custom Outbounds</CardTitle>
          <CardDescription>
            Configure DoH/DoT bootstrap DNS, multi-WAN routing marks, private-IP policies, and upstream Hysteria 2 relay outbounds.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="grid gap-4 md:grid-cols-3">
            <div className="space-y-1.5">
              <Label>Bootstrap DNS Server</Label>
              <Input
                value={dnsRoute.dns_server}
                onChange={(e) => {
                  setDnsRoute({ ...dnsRoute, dns_server: e.target.value })
                }}
                placeholder="https://223.5.5.5:443/dns-query"
              />
            </div>
            <div className="space-y-1.5">
              <Label>DNS TLS Server Name (SNI)</Label>
              <Input
                value={dnsRoute.dns_server_name}
                onChange={(e) => {
                  setDnsRoute({ ...dnsRoute, dns_server_name: e.target.value })
                }}
                placeholder="dns.alidns.com"
              />
            </div>
            <div className="space-y-1.5">
              <Label>DNS Strategy</Label>
              <Select
                value={dnsRoute.dns_strategy || 'default'}
                onValueChange={(v) => {
                  setDnsRoute({ ...dnsRoute, dns_strategy: v === 'default' ? '' : v })
                }}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="default">Default (dual-stack)</SelectItem>
                  <SelectItem value="prefer_ipv4">prefer_ipv4</SelectItem>
                  <SelectItem value="ipv4_only">ipv4_only</SelectItem>
                  <SelectItem value="prefer_ipv6">prefer_ipv6</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className="flex flex-wrap gap-6">
            <label className="flex items-center gap-2 text-sm">
              <Switch
                checked={dnsRoute.hijack_dns}
                onCheckedChange={(checked) => {
                  setDnsRoute({ ...dnsRoute, hijack_dns: checked })
                }}
              />
              <span>Hijack DNS traffic</span>
            </label>
            <label className="flex items-center gap-2 text-sm">
              <Switch
                checked={dnsRoute.auto_detect_interface}
                onCheckedChange={(checked) => {
                  setDnsRoute({ ...dnsRoute, auto_detect_interface: checked })
                }}
              />
              <span>auto_detect_interface (disable on multi-WAN)</span>
            </label>
            <label className="flex items-center gap-2 text-sm">
              <Switch
                checked={dnsRoute.reject_private_ip}
                onCheckedChange={(checked) => {
                  setDnsRoute({ ...dnsRoute, reject_private_ip: checked })
                }}
              />
              <span>Reject private IPs (disable for CERNET/LAN)</span>
            </label>
          </div>

          <div className="space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <Label className="text-sm font-semibold">Custom Outbounds</Label>
              <div className="flex gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    setOutbounds((prev) => [
                      ...prev,
                      { type: 'direct', tag: 'direct-campus', bind_interface: 'wan', routing_mark: 262144 },
                    ])
                  }}
                >
                  <Plus className="size-3.5" />
                  Add Direct Outbound
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    setOutbounds((prev) => [
                      ...prev,
                      {
                        type: 'hysteria2',
                        tag: 'jp-hy2',
                        server: '',
                        server_port: 40001,
                        password: '',
                        insecure: false,
                        alpn: ['h3'],
                      },
                    ])
                  }}
                >
                  <Plus className="size-3.5" />
                  Add Hysteria 2 Relay
                </Button>
              </div>
            </div>

            {outbounds.map((ob, idx) => (
              <div key={`${ob.type}-${idx}`} className="space-y-3 rounded-lg border p-3">
                <div className="flex items-center justify-between">
                  <Badge variant="outline">{ob.type}</Badge>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    onClick={() => {
                      setOutbounds((prev) => prev.filter((_, i) => i !== idx))
                    }}
                  >
                    <Trash2 className="text-destructive size-4" />
                  </Button>
                </div>

                <div className="grid gap-3 md:grid-cols-3">
                  <div className="space-y-1">
                    <Label>Tag</Label>
                    <Input
                      value={ob.tag}
                      onChange={(e) => {
                        setOutbounds((prev) =>
                          prev.map((item, i) => (i === idx ? { ...item, tag: e.target.value } : item)),
                        )
                      }}
                    />
                  </div>
                  <div className="space-y-1">
                    <Label>Bind Interface (optional)</Label>
                    <Input
                      value={ob.bind_interface ?? ''}
                      onChange={(e) => {
                        setOutbounds((prev) =>
                          prev.map((item, i) =>
                            i === idx ? { ...item, bind_interface: e.target.value || undefined } : item,
                          ),
                        )
                      }}
                      placeholder="wan"
                    />
                  </div>
                  <div className="space-y-1">
                    <Label>Routing Mark (optional)</Label>
                    <Input
                      type="number"
                      value={ob.routing_mark ?? ''}
                      onChange={(e) => {
                        const n = Number(e.target.value)
                        setOutbounds((prev) =>
                          prev.map((item, i) =>
                            i === idx
                              ? { ...item, routing_mark: e.target.value && Number.isFinite(n) ? n : undefined }
                              : item,
                          ),
                        )
                      }}
                      placeholder="262144"
                    />
                  </div>
                </div>

                {ob.type === 'hysteria2' && (
                  <div className="grid gap-3 md:grid-cols-4">
                    <div className="space-y-1">
                      <Label>Upstream Server</Label>
                      <Input
                        value={ob.server ?? ''}
                        onChange={(e) => {
                          setOutbounds((prev) =>
                            prev.map((item, i) => (i === idx ? { ...item, server: e.target.value } : item)),
                          )
                        }}
                        placeholder="arts-jp-hnd-01.proxy.restart.org.cn"
                      />
                    </div>
                    <div className="space-y-1">
                      <Label>Port</Label>
                      <Input
                        type="number"
                        value={ob.server_port ?? 40001}
                        onChange={(e) => {
                          const n = Number(e.target.value)
                          setOutbounds((prev) =>
                            prev.map((item, i) =>
                              i === idx ? { ...item, server_port: Number.isFinite(n) ? n : 40001 } : item,
                            ),
                          )
                        }}
                      />
                    </div>
                    <div className="space-y-1">
                      <Label>Relay Password</Label>
                      <Input
                        value={ob.password ?? ''}
                        onChange={(e) => {
                          setOutbounds((prev) =>
                            prev.map((item, i) => (i === idx ? { ...item, password: e.target.value } : item)),
                          )
                        }}
                      />
                    </div>
                    <div className="space-y-1">
                      <Label>TLS SNI (optional)</Label>
                      <Input
                        value={ob.server_name ?? ''}
                        onChange={(e) => {
                          setOutbounds((prev) =>
                            prev.map((item, i) =>
                              i === idx ? { ...item, server_name: e.target.value || undefined } : item,
                            ),
                          )
                        }}
                      />
                    </div>
                    <label className="flex items-center gap-2 text-sm md:col-span-4">
                      <Switch
                        checked={ob.insecure ?? false}
                        onCheckedChange={(checked) => {
                          setOutbounds((prev) =>
                            prev.map((item, i) => (i === idx ? { ...item, insecure: checked } : item)),
                          )
                        }}
                      />
                      <span>Skip TLS certificate verification (insecure)</span>
                    </label>
                  </div>
                )}
              </div>
            ))}
          </div>

          <Button
            disabled={saveRoutingMutation.isPending}
            onClick={() => {
              saveRoutingMutation.mutate()
            }}
          >
            <Save className="size-4" />
            Save &amp; Push DNS / Outbounds
          </Button>
        </CardContent>
      </Card>
    </div>
  )
}
