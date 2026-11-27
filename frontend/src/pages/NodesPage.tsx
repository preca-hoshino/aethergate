import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { type ColumnDef } from '@tanstack/react-table'
import { Copy, Plus, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import { formatBytes } from '@/lib/formatters'
import { useAppStore } from '@/stores/use-app-store'
import { useLiveMetricsStore } from '@/stores/use-live-metrics-store'
import type { NodeSummary } from '@/types/api'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { DataTable, DataTableColumnHeader } from '@/components/data-table'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
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

interface BootstrapResult {
  node_id: string
  name: string
  hmac_secret: string
}

export function NodesPage() {
  const { t } = useTranslation()
  const month = useAppStore((s) => s.selectedMonth)
  const liveNodes = useLiveMetricsStore((s) => s.nodes)
  const queryClient = useQueryClient()

  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [host, setHost] = useState('')
  const [region, setRegion] = useState('JP')
  const [parentNodeId, setParentNodeId] = useState('')
  const [bootstrap, setBootstrap] = useState<BootstrapResult | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<NodeSummary | null>(null)

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
        parent_node_id: parentNodeId || null,
      }),
    onSuccess: (res) => {
      setBootstrap(res)
      setName('')
      setHost('')
      setParentNodeId('')
      void queryClient.invalidateQueries({ queryKey: ['nodes'] })
      toast.success(t('nodes.nodeCreated', { name: res.name }))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete<undefined>(`/api/v1/nodes/${id}`),
    onSuccess: () => {
      setDeleteTarget(null)
      void queryClient.invalidateQueries({ queryKey: ['nodes'] })
      toast.success(t('nodes.nodeDeleted'))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const columns = useMemo<ColumnDef<NodeSummary>[]>(
    () => [
      {
        id: 'name',
        accessorFn: (row) => `${row.name} ${row.region}`,
        meta: { label: t('nodes.columns.name') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('nodes.columns.name')} />,
        cell: ({ row }) => {
          const node = row.original
          return (
            <div>
              <div className="flex items-center gap-2 font-medium">
                <Link to="/nodes/$nodeId" params={{ nodeId: node.id }} className="hover:underline">
                  {node.name}
                </Link>
                {node.parent_node_id && <Badge variant="outline">{t('nodes.logicalBadge')}</Badge>}
              </div>
              <div className="text-muted-foreground text-xs">{node.region}</div>
            </div>
          )
        },
      },
      {
        accessorKey: 'host',
        meta: { label: t('nodes.columns.host') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('nodes.columns.host')} />,
        cell: ({ row }) => <span className="font-mono text-xs">{row.original.host}</span>,
      },
      {
        id: 'status',
        accessorFn: (row) => liveNodes[row.id]?.status ?? row.status,
        meta: { label: t('nodes.columns.status') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('nodes.columns.status')} />,
        filterFn: (row, id, value: string[]) => value.includes(String(row.getValue(id))),
        cell: ({ row }) => {
          const status = liveNodes[row.original.id]?.status ?? row.original.status
          const variant = status === 'online' ? 'success' : status === 'degraded' ? 'warning' : 'secondary'
          const label =
            status === 'online'
              ? t('common.online')
              : status === 'degraded'
                ? t('common.degraded')
                : t('common.offline')
          return <Badge variant={variant}>{label}</Badge>
        },
      },
      {
        accessorKey: 'inbound_count',
        meta: { label: t('nodes.columns.inbounds') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('nodes.columns.inbounds')} />,
        cell: ({ row }) => <span className="font-mono text-xs">{row.original.inbound_count}</span>,
      },
      {
        id: 'cpu_ram',
        accessorFn: (row) => liveNodes[row.id]?.cpu_pct ?? row.cpu_pct,
        meta: { label: t('nodes.columns.cpuRam') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('nodes.columns.cpuRam')} />,
        cell: ({ row }) => {
          const live = liveNodes[row.original.id]
          const cpu = live?.cpu_pct ?? row.original.cpu_pct
          const mem = live?.mem_pct ?? row.original.mem_pct
          return (
            <span className="font-mono text-xs">
              {cpu.toFixed(1)}% / {mem.toFixed(1)}%
            </span>
          )
        },
      },
      {
        id: 'active_connections',
        accessorFn: (row) => liveNodes[row.id]?.active_connections ?? row.active_connections,
        meta: { label: t('nodes.columns.connections') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('nodes.columns.connections')} />,
        cell: ({ row }) => (
          <span className="font-mono text-xs">
            {liveNodes[row.original.id]?.active_connections ?? row.original.active_connections}
          </span>
        ),
      },
      {
        id: 'month_traffic',
        accessorFn: (row) => row.month_uplink_bytes + row.month_downlink_bytes,
        meta: { label: t('nodes.columns.monthTraffic') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('nodes.columns.monthTraffic')} />,
        cell: ({ row }) => (
          <span className="font-mono text-xs">
            {formatBytes(row.original.month_uplink_bytes + row.original.month_downlink_bytes)}
          </span>
        ),
      },
      {
        id: 'actions',
        enableHiding: false,
        header: () => <div className="text-right">{t('common.actions')}</div>,
        cell: ({ row }) => {
          const node = row.original
          return (
            <div className="flex justify-end gap-2">
              <Button asChild variant="outline" size="sm">
                <Link to="/nodes/$nodeId" params={{ nodeId: node.id }}>
                  {t('nodes.configure')}
                </Link>
              </Button>
              <Button
                variant="ghost"
                size="icon"
                aria-label={t('common.delete')}
                onClick={() => {
                  setDeleteTarget(node)
                }}
              >
                <Trash2 className="text-destructive size-4" />
              </Button>
            </div>
          )
        },
      },
    ],
    [liveNodes, t],
  )

  const filters = useMemo(
    () => [
      {
        columnId: 'status',
        title: t('common.status'),
        options: [
          { label: t('common.online'), value: 'online' },
          { label: t('common.degraded'), value: 'degraded' },
          { label: t('common.offline'), value: 'offline' },
        ],
      },
    ],
    [t],
  )

  const handleCloseDialog = (nextOpen: boolean) => {
    setOpen(nextOpen)
    if (!nextOpen) {
      setBootstrap(null)
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{t('nodes.title')}</h1>
          <p className="text-muted-foreground text-sm">{t('nodes.subtitle')}</p>
        </div>

        <Dialog open={open} onOpenChange={handleCloseDialog}>
          <DialogTrigger asChild>
            <Button>
              <Plus className="size-4" />
              <span>{t('nodes.addNode')}</span>
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{t('nodes.dialogTitle')}</DialogTitle>
              <DialogDescription>{t('nodes.dialogDesc')}</DialogDescription>
            </DialogHeader>

            {bootstrap ? (
              <div className="space-y-3 text-sm">
                <p className="font-medium text-emerald-600 dark:text-emerald-400">
                  {t('nodes.snippetNotice')}
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
                    toast.success(t('nodes.copiedSnippet'))
                  }}
                >
                  <Copy className="size-3.5" />
                  <span>{t('nodes.copySnippet')}</span>
                </Button>
              </div>
            ) : (
              <div className="space-y-4">
                <div className="space-y-1.5">
                  <Label>{t('nodes.form.name')}</Label>
                  <Input
                    placeholder={t('nodes.form.namePlaceholder')}
                    value={name}
                    onChange={(e) => {
                      setName(e.target.value)
                    }}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{t('nodes.form.host')}</Label>
                  <Input
                    placeholder={t('nodes.form.hostPlaceholder')}
                    value={host}
                    onChange={(e) => {
                      setHost(e.target.value)
                    }}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{t('nodes.form.region')}</Label>
                  <Input
                    placeholder={t('nodes.form.regionPlaceholder')}
                    value={region}
                    onChange={(e) => {
                      setRegion(e.target.value)
                    }}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{t('nodes.form.parentNode')}</Label>
                  <Input
                    placeholder={t('nodes.form.parentNodePlaceholder')}
                    value={parentNodeId}
                    onChange={(e) => {
                      setParentNodeId(e.target.value)
                    }}
                  />
                </div>
              </div>
            )}

            <DialogFooter>
              {bootstrap ? (
                <Button
                  onClick={() => {
                    handleCloseDialog(false)
                  }}
                >
                  {t('common.done')}
                </Button>
              ) : (
                <>
                  <Button
                    variant="outline"
                    onClick={() => {
                      handleCloseDialog(false)
                    }}
                  >
                    {t('common.cancel')}
                  </Button>
                  <Button
                    disabled={createMutation.isPending || !name.trim() || !host.trim()}
                    onClick={() => {
                      createMutation.mutate()
                    }}
                  >
                    {t('nodes.createNode')}
                  </Button>
                </>
              )}
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      <DataTable
        columns={columns}
        data={nodes}
        isLoading={isLoading}
        searchPlaceholder={t('nodes.searchPlaceholder')}
        filters={filters}
        emptyMessage={t('nodes.empty')}
      />

      <ConfirmDialog
        open={deleteTarget !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setDeleteTarget(null)
        }}
        title={t('nodes.deleteConfirmTitle', { name: deleteTarget?.name ?? '' })}
        desc={t('nodes.deleteConfirmDesc')}
        confirmText={t('common.delete')}
        destructive
        isLoading={deleteMutation.isPending}
        handleConfirm={() => {
          if (deleteTarget) {
            deleteMutation.mutate(deleteTarget.id)
          }
        }}
      />
    </div>
  )
}
