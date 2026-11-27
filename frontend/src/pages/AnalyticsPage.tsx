import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { type ColumnDef } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'
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
import { DataTable, DataTableColumnHeader } from '@/components/data-table'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'

export function AnalyticsPage() {
  const { t } = useTranslation()
  const month = useAppStore((s) => s.selectedMonth)

  const { data: users = [], isLoading: usersLoading } = useQuery({
    queryKey: ['analytics-users', month],
    queryFn: () => api.get<UserMonthlyTraffic[]>(`/api/v1/analytics/users?month=${month}`),
  })

  const { data: nodes = [], isLoading: nodesLoading } = useQuery({
    queryKey: ['analytics-nodes', month],
    queryFn: () => api.get<NodeMonthlyTraffic[]>(`/api/v1/analytics/nodes?month=${month}`),
  })

  const { data: matrix = [], isLoading: matrixLoading } = useQuery({
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

  const userColumns = useMemo<ColumnDef<UserMonthlyTraffic>[]>(
    () => [
      {
        id: 'user',
        accessorFn: (row) => `${row.display_name} ${row.proxy_username}`,
        meta: { label: t('analytics.columns.user') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.user')} />,
        cell: ({ row }) => (
          <div>
            <div className="font-medium">{row.original.display_name}</div>
            <div className="text-muted-foreground font-mono text-xs">{row.original.proxy_username}</div>
          </div>
        ),
      },
      {
        accessorKey: 'uplink_bytes',
        meta: { label: t('analytics.columns.uplink') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.uplink')} />,
        cell: ({ row }) => <span className="font-mono text-xs">{formatBytes(row.original.uplink_bytes)}</span>,
      },
      {
        accessorKey: 'downlink_bytes',
        meta: { label: t('analytics.columns.downlink') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('analytics.columns.downlink')} />
        ),
        cell: ({ row }) => <span className="font-mono text-xs">{formatBytes(row.original.downlink_bytes)}</span>,
      },
      {
        id: 'total_bytes',
        accessorFn: (row) => row.uplink_bytes + row.downlink_bytes,
        meta: { label: t('analytics.columns.total') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.total')} />,
        cell: ({ row }) => (
          <span className="font-mono text-xs font-medium">
            {formatBytes(row.original.uplink_bytes + row.original.downlink_bytes)}
          </span>
        ),
      },
      {
        accessorKey: 'peak_downlink_bps',
        meta: { label: t('analytics.columns.peakDownlink') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('analytics.columns.peakDownlink')} />
        ),
        cell: ({ row }) => (
          <span className="font-mono text-xs">{formatBps(row.original.peak_downlink_bps)}</span>
        ),
      },
    ],
    [t],
  )

  const nodeColumns = useMemo<ColumnDef<NodeMonthlyTraffic>[]>(
    () => [
      {
        accessorKey: 'node_name',
        meta: { label: t('analytics.columns.node') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.node')} />,
        cell: ({ row }) => <span className="font-medium">{row.original.node_name}</span>,
      },
      {
        accessorKey: 'region',
        meta: { label: t('analytics.columns.region') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.region')} />,
        cell: ({ row }) => row.original.region,
      },
      {
        accessorKey: 'uplink_bytes',
        meta: { label: t('analytics.columns.uplink') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.uplink')} />,
        cell: ({ row }) => <span className="font-mono text-xs">{formatBytes(row.original.uplink_bytes)}</span>,
      },
      {
        accessorKey: 'downlink_bytes',
        meta: { label: t('analytics.columns.downlink') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('analytics.columns.downlink')} />
        ),
        cell: ({ row }) => <span className="font-mono text-xs">{formatBytes(row.original.downlink_bytes)}</span>,
      },
      {
        id: 'total_bytes',
        accessorFn: (row) => row.uplink_bytes + row.downlink_bytes,
        meta: { label: t('analytics.columns.total') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.total')} />,
        cell: ({ row }) => (
          <span className="font-mono text-xs font-medium">
            {formatBytes(row.original.uplink_bytes + row.original.downlink_bytes)}
          </span>
        ),
      },
      {
        accessorKey: 'peak_downlink_bps',
        meta: { label: t('analytics.columns.peakDownlink') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('analytics.columns.peakDownlink')} />
        ),
        cell: ({ row }) => (
          <span className="font-mono text-xs">{formatBps(row.original.peak_downlink_bps)}</span>
        ),
      },
    ],
    [t],
  )

  const matrixColumns = useMemo<ColumnDef<UserNodeMatrixRow>[]>(
    () => [
      {
        accessorKey: 'display_name',
        meta: { label: t('analytics.columns.user') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.user')} />,
        cell: ({ row }) => row.original.display_name,
      },
      {
        accessorKey: 'node_name',
        meta: { label: t('analytics.columns.node') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.node')} />,
        cell: ({ row }) => row.original.node_name,
      },
      {
        accessorKey: 'uplink_bytes',
        meta: { label: t('analytics.columns.uplink') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.uplink')} />,
        cell: ({ row }) => <span className="font-mono text-xs">{formatBytes(row.original.uplink_bytes)}</span>,
      },
      {
        accessorKey: 'downlink_bytes',
        meta: { label: t('analytics.columns.downlink') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('analytics.columns.downlink')} />
        ),
        cell: ({ row }) => <span className="font-mono text-xs">{formatBytes(row.original.downlink_bytes)}</span>,
      },
      {
        id: 'total_bytes',
        accessorFn: (row) => row.uplink_bytes + row.downlink_bytes,
        meta: { label: t('analytics.columns.total') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('analytics.columns.total')} />,
        cell: ({ row }) => (
          <span className="font-mono text-xs font-medium">
            {formatBytes(row.original.uplink_bytes + row.original.downlink_bytes)}
          </span>
        ),
      },
    ],
    [t],
  )

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{t('analytics.title', { month })}</h1>
        <p className="text-muted-foreground text-sm">{t('analytics.subtitle')}</p>
      </div>

      <Tabs defaultValue="users" className="space-y-4">
        <TabsList>
          <TabsTrigger value="users">{t('analytics.tabs.users')}</TabsTrigger>
          <TabsTrigger value="nodes">{t('analytics.tabs.nodes')}</TabsTrigger>
          <TabsTrigger value="matrix">{t('analytics.tabs.matrix')}</TabsTrigger>
          <TabsTrigger value="protocols">{t('analytics.tabs.protocols')}</TabsTrigger>
        </TabsList>

        <TabsContent value="users">
          <Card>
            <CardHeader>
              <CardTitle>{t('analytics.cards.usersTitle')}</CardTitle>
            </CardHeader>
            <CardContent>
              <DataTable
                columns={userColumns}
                data={users}
                isLoading={usersLoading}
                searchPlaceholder={t('analytics.searchUsersPlaceholder')}
              />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="nodes">
          <Card>
            <CardHeader>
              <CardTitle>{t('analytics.cards.nodesTitle')}</CardTitle>
            </CardHeader>
            <CardContent>
              <DataTable
                columns={nodeColumns}
                data={nodes}
                isLoading={nodesLoading}
                searchPlaceholder={t('analytics.searchNodesPlaceholder')}
              />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="matrix">
          <Card>
            <CardHeader>
              <CardTitle>{t('analytics.cards.matrixTitle')}</CardTitle>
              <CardDescription>{t('analytics.cards.matrixDesc')}</CardDescription>
            </CardHeader>
            <CardContent>
              <DataTable
                columns={matrixColumns}
                data={matrix}
                isLoading={matrixLoading}
                searchPlaceholder={t('analytics.searchMatrixPlaceholder')}
              />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="protocols">
          <Card>
            <CardHeader>
              <CardTitle>{t('analytics.cards.protocolsTitle')}</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {protoChart.length > 0 ? (
                <>
                  <div className="h-72">
                    <ResponsiveContainer width="100%" height="100%">
                      <BarChart data={protoChart}>
                        <CartesianGrid strokeDasharray="3 3" className="stroke-border" vertical={false} />
                        <XAxis dataKey="protocol" stroke="#888888" fontSize={12} tickLine={false} axisLine={false} />
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
                  </div>
                  <div className="flex flex-wrap gap-3">
                    {protocols.map((p) => (
                      <Badge key={p.protocol} variant="outline" className="font-mono">
                        {p.protocol}: {formatBytes(p.uplink_bytes + p.downlink_bytes)}
                      </Badge>
                    ))}
                  </div>
                </>
              ) : (
                <div className="text-muted-foreground py-10 text-center text-sm">
                  {t('analytics.emptyProtocols', { month })}
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  )
}
