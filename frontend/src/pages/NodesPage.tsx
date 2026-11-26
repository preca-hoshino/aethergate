import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { Copy, Plus, Server, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import { formatBytes } from '@/lib/formatters'
import { useAppStore } from '@/stores/use-app-store'
import { useLiveMetricsStore } from '@/stores/use-live-metrics-store'
import type { NodeSummary } from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'

interface BootstrapResult {
  node_id: string
  name: string
  hmac_secret: string
}

export function NodesPage() {
  const month = useAppStore((s) => s.selectedMonth)
  const liveNodes = useLiveMetricsStore((s) => s.nodes)
  const queryClient = useQueryClient()

  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [host, setHost] = useState('')
  const [region, setRegion] = useState('JP')
  const [bootstrap, setBootstrap] = useState<BootstrapResult | null>(null)

  const { data: nodes = [], isLoading } = useQuery({
    queryKey: ['nodes', month],
    queryFn: () => api.get<NodeSummary[]>(`/api/v1/nodes?month=${month}`),
  })

  const createMutation = useMutation({
    mutationFn: () =>
      api.post<BootstrapResult>('/api/v1/nodes', {
        name,
        host,
        region,
      }),
    onSuccess: (res) => {
      setBootstrap(res)
      setName('')
      setHost('')
      void queryClient.invalidateQueries({ queryKey: ['nodes'] })
      toast.success(`Node "${res.name}" created`)
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete<undefined>(`/api/v1/nodes/${id}`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['nodes'] })
      toast.success('Node deleted')
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Nodes</h1>
          <p className="text-muted-foreground text-sm">
            Manage edge servers, 4-protocol inbounds (VLESS Reality, Hysteria 2, TUIC, AnyTLS), and HMAC secrets.
          </p>
        </div>

        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button>
              <Plus className="size-4" />
              Add Node
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Register Edge Node</DialogTitle>
              <DialogDescription>
                Creates a node record and generates a one-time HMAC-SHA256 secret for `aethergate-agent`.
              </DialogDescription>
            </DialogHeader>

            {bootstrap ? (
              <div className="space-y-3 text-sm">
                <p className="font-medium text-emerald-600 dark:text-emerald-400">
                  Save this `config.agent.toml` snippet now (secret will not be shown again):
                </p>
                <pre className="bg-muted overflow-x-auto rounded-md p-3 font-mono text-xs">
                  {`node_id = "${bootstrap.node_id}"\nhmac_secret = "${bootstrap.hmac_secret}"\nserver_url = "${window.location.origin}"`}
                </pre>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    void navigator.clipboard.writeText(
                      `node_id = "${bootstrap.node_id}"\nhmac_secret = "${bootstrap.hmac_secret}"\nserver_url = "${window.location.origin}"`,
                    )
                    toast.success('Copied agent config snippet')
                  }}
                >
                  <Copy className="size-3.5" />
                  Copy Snippet
                </Button>
              </div>
            ) : (
              <div className="space-y-4">
                <div className="space-y-1.5">
                  <Label>Node Name</Label>
                  <Input placeholder="Tokyo-Edge-01" value={name} onChange={(e) => { setName(e.target.value); }} />
                </div>
                <div className="space-y-1.5">
                  <Label>Public Host / IP</Label>
                  <Input placeholder="jp01.example.com" value={host} onChange={(e) => { setHost(e.target.value); }} />
                </div>
                <div className="space-y-1.5">
                  <Label>Region</Label>
                  <Input placeholder="JP" value={region} onChange={(e) => { setRegion(e.target.value); }} />
                </div>
              </div>
            )}

            <DialogFooter>
              {bootstrap ? (
                <Button
                  onClick={() => {
                    setBootstrap(null)
                    setOpen(false)
                  }}
                >
                  Done
                </Button>
              ) : (
                <Button
                  disabled={createMutation.isPending || !name || !host}
                  onClick={() => {
                    createMutation.mutate()
                  }}
                >
                  Create Node
                </Button>
              )}
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Server className="size-4" />
            Node Inventory ({nodes.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Host</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Inbounds</TableHead>
                <TableHead>CPU / RAM</TableHead>
                <TableHead>Connections</TableHead>
                <TableHead>Month Traffic</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {nodes.map((node) => {
                const live = liveNodes[node.id]
                const status = live?.status ?? node.status
                return (
                  <TableRow key={node.id}>
                    <TableCell className="font-medium">
                      <Link to="/nodes/$nodeId" params={{ nodeId: node.id }} className="hover:underline">
                        {node.name}
                      </Link>
                      <div className="text-muted-foreground text-xs">{node.region}</div>
                    </TableCell>
                    <TableCell className="font-mono text-xs">{node.host}</TableCell>
                    <TableCell>
                      <Badge variant={status === 'online' ? 'success' : 'secondary'}>{status}</Badge>
                    </TableCell>
                    <TableCell>{node.inbound_count}</TableCell>
                    <TableCell className="font-mono text-xs">
                      {(live?.cpu_pct ?? node.cpu_pct).toFixed(1)}% / {(live?.mem_pct ?? node.mem_pct).toFixed(1)}%
                    </TableCell>
                    <TableCell className="font-mono text-xs">
                      {live?.active_connections ?? node.active_connections}
                    </TableCell>
                    <TableCell className="font-mono text-xs">
                      {formatBytes(node.month_uplink_bytes + node.month_downlink_bytes)}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex justify-end gap-2">
                        <Button asChild variant="outline" size="sm">
                          <Link to="/nodes/$nodeId" params={{ nodeId: node.id }}>
                            Configure
                          </Link>
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => {
                            deleteMutation.mutate(node.id)
                          }}
                        >
                          <Trash2 className="text-destructive size-4" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                )
              })}
              {!isLoading && nodes.length === 0 && (
                <TableRow>
                  <TableCell colSpan={8} className="text-muted-foreground py-8 text-center">
                    No nodes registered yet. Click &ldquo;Add Node&rdquo; to bootstrap your first edge node.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  )
}
