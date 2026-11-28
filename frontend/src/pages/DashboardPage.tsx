import { useQuery } from '@tanstack/react-query'
import { Activity, ArrowDownToLine, ArrowUpFromLine, Server, Users } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { api } from '@/lib/api-client'
import { formatBytes, formatTimeShort } from '@/lib/formatters'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/stores/use-app-store'
import { useLiveMetricsStore } from '@/stores/use-live-metrics-store'
import type { AnalyticsOverview, TimeSeriesBucket, UserMonthlyTraffic } from '@/types/api'
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'

export function DashboardPage() {
  const { t, i18n } = useTranslation()
  const month = useAppStore((s) => s.selectedMonth)
  const liveNodes = useLiveMetricsStore((s) => s.nodes)

  const { data: overview, isLoading: overviewLoading } = useQuery({
    queryKey: ['analytics-overview', month],
    queryFn: () => api.get<AnalyticsOverview>(`/api/v1/analytics/overview?month=${month}`),
  })

  const { data: timeseries = [] } = useQuery({
    queryKey: ['analytics-timeseries'],
    queryFn: () => api.get<TimeSeriesBucket[]>('/api/v1/analytics/timeseries?hours=24'),
  })

  const { data: topUsers = [], isLoading: usersLoading } = useQuery({
    queryKey: ['analytics-users', month],
    queryFn: () => api.get<UserMonthlyTraffic[]>(`/api/v1/analytics/users?month=${month}`),
  })

  const totalTraffic =
    (overview?.total_uplink_bytes ?? 0) + (overview?.total_downlink_bytes ?? 0)
  const liveConnectionsTotal =
    Object.keys(liveNodes).length > 0
      ? Object.values(liveNodes).reduce((acc, n) => acc + n.active_connections, 0)
      : (overview?.live_connections ?? 0)

  const chartData = timeseries.map((b) => ({
    label: formatTimeShort(b.bucket, i18n.language),
    uplinkGiB: Number((b.uplink_bytes / 1024 ** 3).toFixed(3)),
    downlinkGiB: Number((b.downlink_bytes / 1024 ** 3).toFixed(3)),
  }))

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{t('dashboard.title')}</h1>
          <p className="text-muted-foreground text-sm">{t('dashboard.subtitle', { month })}</p>
        </div>
      </div>

      <div className="space-y-4">
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium">{t('dashboard.kpi.monthlyTraffic')}</CardTitle>
              <ArrowDownToLine className="text-muted-foreground size-4" />
            </CardHeader>
            <CardContent>
              {overviewLoading ? (
                <Skeleton className="h-8 w-28" />
              ) : (
                <div className="text-2xl font-bold">{formatBytes(totalTraffic)}</div>
              )}
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
              <CardTitle className="text-sm font-medium">{t('dashboard.kpi.onlineNodes')}</CardTitle>
              <Server className="text-muted-foreground size-4" />
            </CardHeader>
            <CardContent>
              {overviewLoading ? (
                <Skeleton className="h-8 w-20" />
              ) : (
                <div className="text-2xl font-bold">
                  {overview?.online_nodes ?? 0} / {overview?.total_nodes ?? 0}
                </div>
              )}
              <p className="text-muted-foreground mt-1 text-xs">{t('dashboard.kpi.managedEdgeNodes')}</p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium">{t('dashboard.kpi.activeProxyUsers')}</CardTitle>
              <Users className="text-muted-foreground size-4" />
            </CardHeader>
            <CardContent>
              {overviewLoading ? (
                <Skeleton className="h-8 w-16" />
              ) : (
                <div className="text-2xl font-bold">{overview?.active_proxy_users ?? 0}</div>
              )}
              <p className="text-muted-foreground mt-1 text-xs">
                {t('dashboard.kpi.pendingApprovalCount', {
                  count: overview?.pending_approval_users ?? 0,
                })}
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium">{t('dashboard.kpi.liveConnections')}</CardTitle>
              <Activity className="text-muted-foreground size-4" />
            </CardHeader>
            <CardContent>
              {overviewLoading ? (
                <Skeleton className="h-8 w-16" />
              ) : (
                <div className="text-2xl font-bold">{liveConnectionsTotal}</div>
              )}
              <p className="text-muted-foreground mt-1 text-xs">{t('dashboard.kpi.clashApiSum')}</p>
            </CardContent>
          </Card>
        </div>

        <div className="grid gap-4 lg:grid-cols-7">
          <Card className="lg:col-span-4">
            <CardHeader>
              <CardTitle>{t('dashboard.charts.throughputTitle')}</CardTitle>
              <CardDescription>{t('dashboard.charts.throughputDesc')}</CardDescription>
            </CardHeader>
            <CardContent className="h-80 ps-2">
              {chartData.length > 0 ? (
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={chartData}>
                    <CartesianGrid strokeDasharray="3 3" className="stroke-border" vertical={false} />
                    <XAxis dataKey="label" stroke="#888888" fontSize={12} tickLine={false} axisLine={false} />
                    <YAxis stroke="#888888" fontSize={12} tickLine={false} axisLine={false} />
                    <Tooltip
                      contentStyle={{
                        backgroundColor: 'var(--popover)',
                        borderColor: 'var(--border)',
                        borderRadius: 'var(--radius)',
                        color: 'var(--popover-foreground)',
                        fontSize: '12px',
                      }}
                    />
                    <Bar
                      dataKey="downlinkGiB"
                      name={t('dashboard.charts.downlinkGib')}
                      fill="var(--chart-1)"
                      radius={[4, 4, 0, 0]}
                    />
                    <Bar
                      dataKey="uplinkGiB"
                      name={t('dashboard.charts.uplinkGib')}
                      fill="var(--chart-2)"
                      radius={[4, 4, 0, 0]}
                    />
                  </BarChart>
                </ResponsiveContainer>
              ) : (
                <div className="text-muted-foreground flex h-full items-center justify-center text-sm">
                  {t('dashboard.charts.noTimeseries')}
                </div>
              )}
            </CardContent>
          </Card>

          <Card className="lg:col-span-3">
            <CardHeader>
              <CardTitle>{t('dashboard.topUsers.title')}</CardTitle>
              <CardDescription>{t('dashboard.topUsers.desc')}</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              {usersLoading && (
                <div className="space-y-3">
                  <Skeleton className="h-10 w-full" />
                  <Skeleton className="h-10 w-full" />
                  <Skeleton className="h-10 w-full" />
                </div>
              )}
              {!usersLoading &&
                topUsers.slice(0, 6).map((u) => {
                  const email = u.email?.trim()
                  return (
                    <div key={u.user_id} className="flex items-center gap-3 text-sm">
                      <Avatar className="size-9">
                        <AvatarImage
                          src={u.avatar_url ?? undefined}
                          alt={u.display_name}
                          referrerPolicy="no-referrer"
                        />
                        <AvatarFallback>{u.display_name.slice(0, 2).toUpperCase()}</AvatarFallback>
                      </Avatar>
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-medium leading-none">{u.display_name}</div>
                        <div
                          className={cn(
                            'text-muted-foreground mt-1 truncate text-xs',
                            !email && 'font-mono',
                          )}
                        >
                          {email || u.proxy_username}
                        </div>
                      </div>
                      <div className="font-mono text-xs font-medium">
                        {formatBytes(u.uplink_bytes + u.downlink_bytes)}
                      </div>
                    </div>
                  )
                })}
              {!usersLoading && topUsers.length === 0 && (
                <p className="text-muted-foreground py-8 text-center text-sm">
                  {t('dashboard.topUsers.empty', { month })}
                </p>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}
