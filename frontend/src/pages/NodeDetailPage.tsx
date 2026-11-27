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
  const [addProto, setAddProto] = useState<ProtocolType>('vless_reality')

  useEffect(() => {
    if (node) {
      setInbounds(node.inbounds)
      setTls(node.tls_profile)
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
    </div>
  )
}
