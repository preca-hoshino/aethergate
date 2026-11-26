import { useQuery } from '@tanstack/react-query'
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { api } from '@/lib/api-client'
import { formatBps, formatBytes } from '@/lib/formatters'
import { useAppStore } from '@/stores/use-app-store'
import type {
  NodeMonthlyTraffic,
  ProtocolMonthlyRow,
  UserMonthlyTraffic,
  UserNodeMatrixRow,
} from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'

export function AnalyticsPage() {
  const month = useAppStore((s) => s.selectedMonth)

  const { data: users = [] } = useQuery({
    queryKey: ['analytics-users', month],
    queryFn: () => api.get<UserMonthlyTraffic[]>(`/api/v1/analytics/users?month=${month}`),
  })

  const { data: nodes = [] } = useQuery({
    queryKey: ['analytics-nodes', month],
    queryFn: () => api.get<NodeMonthlyTraffic[]>(`/api/v1/analytics/nodes?month=${month}`),
  })

  const { data: matrix = [] } = useQuery({
    queryKey: ['analytics-matrix', month],
    queryFn: () => api.get<UserNodeMatrixRow[]>(`/api/v1/analytics/matrix?month=${month}`),
  })

  const { data: protocols = [] } = useQuery({
    queryKey: ['analytics-protocols', month],
    queryFn: () => api.get<ProtocolMonthlyRow[]>(`/api/v1/analytics/protocols?month=${month}`),
  })

  const protoChart = protocols.map((p) => ({
    protocol: p.protocol,
    uplinkGiB: Number((p.uplink_bytes / 1024 ** 3).toFixed(3)),
    downlinkGiB: Number((p.downlink_bytes / 1024 ** 3).toFixed(3)),
  }))

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">3D Monthly Analytics ({month})</h1>
        <p className="text-muted-foreground text-sm">
          Traffic &amp; peak bandwidth across User, Node, User × Node Matrix, and 4-Protocol dimensions.
        </p>
      </div>

      <Tabs defaultValue="users" className="space-y-4">
        <TabsList>
          <TabsTrigger value="users">Users</TabsTrigger>
          <TabsTrigger value="nodes">Nodes</TabsTrigger>
          <TabsTrigger value="matrix">User × Node Matrix</TabsTrigger>
          <TabsTrigger value="protocols">Protocols</TabsTrigger>
        </TabsList>

        <TabsContent value="users">
          <Card>
            <CardHeader>
              <CardTitle>User Monthly Traffic</CardTitle>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>User</TableHead>
                    <TableHead>Uplink</TableHead>
                    <TableHead>Downlink</TableHead>
                    <TableHead>Total</TableHead>
                    <TableHead>Peak Downlink</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {users.map((u) => (
                    <TableRow key={u.user_id}>
                      <TableCell>
                        <div className="font-medium">{u.display_name}</div>
                        <div className="text-muted-foreground font-mono text-xs">{u.proxy_username}</div>
                      </TableCell>
                      <TableCell className="font-mono text-xs">{formatBytes(u.uplink_bytes)}</TableCell>
                      <TableCell className="font-mono text-xs">{formatBytes(u.downlink_bytes)}</TableCell>
                      <TableCell className="font-mono text-xs font-medium">
                        {formatBytes(u.uplink_bytes + u.downlink_bytes)}
                      </TableCell>
                      <TableCell className="font-mono text-xs">{formatBps(u.peak_downlink_bps)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="nodes">
          <Card>
            <CardHeader>
              <CardTitle>Node Monthly Traffic</CardTitle>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Node</TableHead>
                    <TableHead>Region</TableHead>
                    <TableHead>Uplink</TableHead>
                    <TableHead>Downlink</TableHead>
                    <TableHead>Total</TableHead>
                    <TableHead>Peak Downlink</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {nodes.map((n) => (
                    <TableRow key={n.node_id}>
                      <TableCell className="font-medium">{n.node_name}</TableCell>
                      <TableCell>{n.region}</TableCell>
                      <TableCell className="font-mono text-xs">{formatBytes(n.uplink_bytes)}</TableCell>
                      <TableCell className="font-mono text-xs">{formatBytes(n.downlink_bytes)}</TableCell>
                      <TableCell className="font-mono text-xs font-medium">
                        {formatBytes(n.uplink_bytes + n.downlink_bytes)}
                      </TableCell>
                      <TableCell className="font-mono text-xs">{formatBps(n.peak_downlink_bps)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="matrix">
          <Card>
            <CardHeader>
              <CardTitle>User × Node Matrix</CardTitle>
              <CardDescription>Exact monthly traffic per user per node</CardDescription>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>User</TableHead>
                    <TableHead>Node</TableHead>
                    <TableHead>Uplink</TableHead>
                    <TableHead>Downlink</TableHead>
                    <TableHead>Total</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {matrix.map((m) => (
                    <TableRow key={`${m.user_id}-${m.node_id}`}>
                      <TableCell>{m.display_name}</TableCell>
                      <TableCell>{m.node_name}</TableCell>
                      <TableCell className="font-mono text-xs">{formatBytes(m.uplink_bytes)}</TableCell>
                      <TableCell className="font-mono text-xs">{formatBytes(m.downlink_bytes)}</TableCell>
                      <TableCell className="font-mono text-xs font-medium">
                        {formatBytes(m.uplink_bytes + m.downlink_bytes)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="protocols">
          <Card>
            <CardHeader>
              <CardTitle>4-Protocol Breakdown (GiB)</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="h-72">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={protoChart}>
                    <CartesianGrid strokeDasharray="3 3" className="stroke-border" />
                    <XAxis dataKey="protocol" fontSize={12} />
                    <YAxis fontSize={12} />
                    <Tooltip />
                    <Bar dataKey="downlinkGiB" name="Downlink (GiB)" fill="var(--chart-1)" radius={[4, 4, 0, 0]} />
                    <Bar dataKey="uplinkGiB" name="Uplink (GiB)" fill="var(--chart-2)" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
              <div className="flex flex-wrap gap-3">
                {protocols.map((p) => (
                  <Badge key={p.protocol} variant="outline" className="font-mono">
                    {p.protocol}: {formatBytes(p.uplink_bytes + p.downlink_bytes)}
                  </Badge>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  )
}
