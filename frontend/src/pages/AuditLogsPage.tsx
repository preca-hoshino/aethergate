import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { type ColumnDef } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'
import { api } from '@/lib/api-client'
import { formatDateTime } from '@/lib/formatters'
import type { AuditLogRow } from '@/types/api'
import { DataTable, DataTableColumnHeader, type DataTableFilterConfig } from '@/components/data-table'
import { Badge } from '@/components/ui/badge'

export function AuditLogsPage() {
  const { t, i18n } = useTranslation()

  const { data: logs = [], isLoading } = useQuery({
    queryKey: ['audit-logs'],
    queryFn: () => api.get<AuditLogRow[]>('/api/v1/audit-logs'),
  })

  const columns = useMemo<ColumnDef<AuditLogRow>[]>(
    () => [
      {
        accessorKey: 'created_at',
        meta: { label: t('auditLogs.columns.time') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('auditLogs.columns.time')} />,
        cell: ({ row }) => (
          <span className="font-mono text-xs">{formatDateTime(row.original.created_at, i18n.language)}</span>
        ),
      },
      {
        id: 'actor',
        accessorFn: (row) => row.actor_name ?? t('common.system'),
        meta: { label: t('auditLogs.columns.actor') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('auditLogs.columns.actor')} />,
        cell: ({ row }) => row.original.actor_name ?? t('common.system'),
      },
      {
        accessorKey: 'action',
        meta: { label: t('auditLogs.columns.action') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('auditLogs.columns.action')} />,
        cell: ({ row }) => (
          <Badge variant="outline" className="font-mono">
            {row.original.action}
          </Badge>
        ),
      },
      {
        accessorKey: 'target_type',
        meta: { label: t('auditLogs.columns.target') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('auditLogs.columns.target')} />,
        filterFn: (row, id, value: string[]) => value.includes(String(row.getValue(id))),
        cell: ({ row }) => {
          const item = row.original
          return (
            <span className="font-mono text-xs">
              {item.target_type} {item.target_id ? `(${item.target_id.slice(0, 8)})` : ''}
            </span>
          )
        },
      },
      {
        id: 'detail',
        accessorFn: (row) => JSON.stringify(row.detail),
        enableSorting: false,
        meta: { label: t('auditLogs.columns.detail') },
        header: () => t('auditLogs.columns.detail'),
        cell: ({ row }) => (
          <div className="text-muted-foreground max-w-xs truncate font-mono text-xs">
            {JSON.stringify(row.original.detail)}
          </div>
        ),
      },
    ],
    [i18n.language, t],
  )

  const targetTypeOptions = useMemo(() => {
    const distinct = Array.from(new Set(logs.map((row) => row.target_type))).sort()
    return distinct.map((val) => ({ label: val, value: val }))
  }, [logs])

  const filters = useMemo<ReadonlyArray<DataTableFilterConfig>>(
    () =>
      targetTypeOptions.length > 0
        ? [
            {
              columnId: 'target_type',
              title: t('auditLogs.targetTypeFilter'),
              options: targetTypeOptions,
            },
          ]
        : [],
    [t, targetTypeOptions],
  )

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{t('auditLogs.title')}</h1>
        <p className="text-muted-foreground text-sm">{t('auditLogs.subtitle')}</p>
      </div>

      <DataTable
        columns={columns}
        data={logs}
        isLoading={isLoading}
        searchPlaceholder={t('auditLogs.searchPlaceholder')}
        filters={filters}
        initialSorting={[{ id: 'created_at', desc: true }]}
      />
    </div>
  )
}
