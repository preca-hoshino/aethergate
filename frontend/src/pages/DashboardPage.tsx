import { useQuery } from '@tanstack/react-query'
import { Activity, ArrowDownToLine, ArrowUpFromLine, Server, Users } from 'lucide-react'
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { api } from '@/lib/api-client'
import { formatBytes } from '@/lib/formatters'
import { useAppStore } from '@/stores/use-app-store'
import { useLiveMetricsStore } from '@/stores/use-live-metrics-store'
import type {
  AnalyticsOverview,
  NodeMonthlyTraffic,
  TimeSeriesBucket,
  UserMonthlyTraffic,
} from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'

export function DashboardPage() {
  const month = useAppStore((s) => s.selectedMonth)
  const liveNodes = useLiveMetricsStore((s) => s.nodes)

  const { data: overview } = useQuery({
    queryKey: ['analytics-overview', month],
    queryFn: () => api.get<AnalyticsOverview>(`/api/v1/analytics/overview?month=${month}`),
  })

  const { data: timeseries = [] } = useQuery({
    queryKey: ['analytics-timeseries'],
    queryFn: () => api.get<TimeSeriesBucket[]>('/api/v1/analytics/timeseries?hours=24'),
  })

  const { data: topUsers = [] } = useQuery({
    queryKey: ['analytics-users', month],
    queryFn: () => api.get<UserMonthlyTraffic[]>(`/api/v1/analytics/users?month=${month}`),
  })

  const { data: topNodes = [] } = useQuery({
    queryKey: ['analytics-nodes', month],
    queryFn: () => api.get<NodeMonthlyTraffic[]>(`/api/v1/analytics/nodes?month=${month}`),
  })

  const totalTraffic =
    (overview?.total_uplink_bytes ?? 0) + (overview?.total_downlink_bytes ?? 0)
  const liveConnectionsTotal =
    Object.keys(liveNodes).length > 0
      ? Object.values(liveNodes).reduce((acc, n) => acc + n.active_connections, 0)
      : (overview?.live_connections ?? 0)

  const chartData = timeseries.map((b) => ({
    label: new Date(b.bucket).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    uplinkGiB: Number((b.uplink_bytes / 1024 ** 3).toFixed(3)),
    downlinkGiB: Number((b.downlink_bytes / 1024 ** 3).toFixed(3)),
  }))

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
          <p className="text-muted-foreground text-sm">
            Natural calendar month summary ({month}) · resets on the 1st of each month.
          </p>
        </div>
      </div>

      <Tabs defaultValue="overview" className="space-y-4">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="nodes">Live Nodes</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-4">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <Card>
              <CardHeader className="flex flex-row items-center justify-between pb-2">
                <CardTitle className="text-sm font-medium">Monthly Traffic</CardTitle>
                <ArrowDownToLine className="text-muted-foreground size-4" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{formatBytes(totalTraffic)}</div>
                <p className="text-muted-foreground mt-1 flex items-center gap-2 text-xs">
                  <span className="inline-flex items-center gap-1">
                    <ArrowUpFromLine className="size-3" />
                    {formatBytes(overview?.total_uplink_bytes ?? 0)}
                  </span>
                  <span>·</span>
                  <span>↓ {formatBytes(overview?.total_downlink_bytes ?? 0)}</span>
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between pb-2">
                <CardTitle className="text-sm font-medium">Online Nodes</CardTitle>
                <Server className="text-muted-foreground size-4" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">
                  {overview?.online_nodes ?? 0} / {overview?.total_nodes ?? 0}
                </div>
                <p className="text-muted-foreground mt-1 text-xs">Managed edge nodes</p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between pb-2">
                <CardTitle className="text-sm font-medium">Active Proxy Users</CardTitle>
                <Users className="text-muted-foreground size-4" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{overview?.active_proxy_users ?? 0}</div>
                <p className="text-muted-foreground mt-1 text-xs">
                  {overview?.pending_approval_users ?? 0} pending approval
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between pb-2">
                <CardTitle className="text-sm font-medium">Live Connections</CardTitle>
                <Activity className="text-muted-foreground size-4" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{liveConnectionsTotal}</div>
                <p className="text-muted-foreground mt-1 text-xs">Real-time Clash API sum</p>
              </CardContent>
            </Card>
          </div>

          <div className="grid gap-4 lg:grid-cols-7">
            <Card className="lg:col-span-4">
              <CardHeader>
                <CardTitle>24h Traffic Throughput (GiB)</CardTitle>
                <CardDescription>Hourly aggregated uplink &amp; downlink volume</CardDescription>
              </CardHeader>
              <CardContent className="h-80">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={chartData}>
                    <CartesianGrid strokeDasharray="3 3" className="stroke-border" />
                    <XAxis dataKey="label" fontSize={12} tickLine={false} axisLine={false} />
                    <YAxis fontSize={12} tickLine={false} axisLine={false} />
                    <Tooltip />
                    <Bar dataKey="downlinkGiB" name="Downlink (GiB)" fill="var(--chart-1)" radius={[4, 4, 0, 0]} />
                    <Bar dataKey="uplinkGiB" name="Uplink (GiB)" fill="var(--chart-2)" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </CardContent>
            </Card>

            <Card className="lg:col-span-3">
              <CardHeader>
                <CardTitle>Top Users ({month})</CardTitle>
                <CardDescription>Highest monthly proxy bandwidth consumers</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                {topUsers.slice(0, 6).map((u) => (
                  <div key={u.user_id} className="flex items-center justify-between gap-2 text-sm">
                    <div className="min-w-0">
                      <div className="truncate font-medium">{u.display_name}</div>
                      <div className="text-muted-foreground truncate text-xs">{u.proxy_username}</div>
                    </div>
                    <div className="text-right font-mono text-xs">
                      {formatBytes(u.uplink_bytes + u.downlink_bytes)}
                    </div>
                  </div>
                ))}
                {topUsers.length === 0 && (
                  <p className="text-muted-foreground text-sm">No traffic recorded for {month} yet.</p>
                )}
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="nodes">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {topNodes.map((n) => {
              const live = liveNodes[n.node_id]
              const status = live?.status ?? n.status
              return (
                <Card key={n.node_id}>
                  <CardHeader className="flex flex-row items-center justify-between pb-2">
                    <div>
                      <CardTitle className="text-base">{n.node_name}</CardTitle>
                      <CardDescription>{n.region}</CardDescription>
                    </div>
                    <Badge variant={status === 'online' ? 'success' : 'secondary'}>{status}</Badge>
                  </CardHeader>
                  <CardContent className="space-y-1 text-xs">
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Monthly Traffic</span>
                      <span className="font-mono">{formatBytes(n.uplink_bytes + n.downlink_bytes)}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">CPU / RAM</span>
                      <span className="font-mono">
                        {(live?.cpu_pct ?? 0).toFixed(1)}% / {(live?.mem_pct ?? 0).toFixed(1)}%
                      </span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Connections</span>
                      <span className="font-mono">{live?.active_connections ?? 0}</span>
                    </div>
                  </CardContent>
                </Card>
              )
            })}
          </div>
        </TabsContent>
      </Tabs>
    </div>
  )
}
