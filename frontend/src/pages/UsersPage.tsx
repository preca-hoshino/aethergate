import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type ColumnDef } from '@tanstack/react-table'
import { Ban, CheckCircle2, Copy, KeyRound, Plus, ShieldCheck, UserX } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import { formatBytes } from '@/lib/formatters'
import { useAppStore } from '@/stores/use-app-store'
import type { NodeGroup, NodeSummary, UserDetail, UserSummary } from '@/types/api'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { DataTable, DataTableColumnHeader, type DataTableFilterConfig } from '@/components/data-table'
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'

interface CreatedUserResult {
  id: string
  proxy_username: string
  proxy_uuid: string
  proxy_password: string
  account_type: 'user' | 'relay'
}

export function UsersPage() {
  const { t } = useTranslation()
  const month = useAppStore((s) => s.selectedMonth)
  const queryClient = useQueryClient()

  const [grantUserId, setGrantUserId] = useState<string | null>(null)
  const [nodeIds, setNodeIds] = useState<string[]>([])
  const [groupIds, setGroupIds] = useState<string[]>([])
  const [kickTarget, setKickTarget] = useState<UserSummary | null>(null)

  const [createOpen, setCreateOpen] = useState(false)
  const [displayName, setDisplayName] = useState('')
  const [accountType, setAccountType] = useState<'relay' | 'user'>('relay')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyUuid, setProxyUuid] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')
  const [createNodeIds, setCreateNodeIds] = useState<string[]>([])
  const [createdUser, setCreatedUser] = useState<CreatedUserResult | null>(null)

  const { data: users = [], isLoading } = useQuery({
    queryKey: ['users', month],
    queryFn: () => api.get<UserSummary[]>(`/api/v1/users?month=${month}`),
  })

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes-list'],
    queryFn: () => api.get<NodeSummary[]>('/api/v1/nodes'),
  })

  const { data: groups = [] } = useQuery({
    queryKey: ['node-groups-list'],
    queryFn: () => api.get<NodeGroup[]>('/api/v1/node-groups'),
  })

  const openGrantsModal = async (userId: string) => {
    setGrantUserId(userId)
    try {
      const detail = await api.get<UserDetail>(`/api/v1/users/${userId}`)
      setNodeIds(detail.node_ids)
      setGroupIds(detail.group_ids)
    } catch {
      setNodeIds([])
      setGroupIds([])
    }
  }

  const approveMutation = useMutation({
    mutationFn: (id: string) =>
      api.post(`/api/v1/users/${id}/approve`, {
        node_ids: nodeIds,
        group_ids: groupIds,
      }),
    onSuccess: () => {
      setGrantUserId(null)
      void queryClient.invalidateQueries({ queryKey: ['users'] })
      toast.success(t('users.toasts.approved'))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const saveGrantsMutation = useMutation({
    mutationFn: (id: string) =>
      api.put(`/api/v1/users/${id}/grants`, { node_ids: nodeIds, group_ids: groupIds }),
    onSuccess: () => {
      setGrantUserId(null)
      void queryClient.invalidateQueries({ queryKey: ['users'] })
      toast.success(t('users.toasts.grantsUpdated'))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const toggleProxyMutation = useMutation({
    mutationFn: ({ id, proxy_enabled }: { id: string; proxy_enabled: boolean }) =>
      api.patch(`/api/v1/users/${id}`, { proxy_enabled }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['users'] })
      toast.success(t('users.toasts.statusUpdated'))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const kickMutation = useMutation({
    mutationFn: (id: string) => api.post(`/api/v1/users/${id}/kick`),
    onSuccess: () => {
      setKickTarget(null)
      toast.success(t('users.toasts.kickDispatched'))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const createUserMutation = useMutation({
    mutationFn: () =>
      api.post<CreatedUserResult>('/api/v1/users', {
        display_name: displayName.trim(),
        account_type: accountType,
        proxy_username: proxyUsername.trim() || undefined,
        proxy_uuid: proxyUuid.trim() || undefined,
        proxy_password: proxyPassword || undefined,
        node_ids: createNodeIds,
      }),
    onSuccess: (res) => {
      setCreatedUser(res)
      void queryClient.invalidateQueries({ queryKey: ['users'] })
      toast.success(
        t('users.toasts.accountCreated', {
          type: res.account_type === 'relay' ? t('users.accountTypes.relay') : t('users.accountTypes.user'),
          username: res.proxy_username,
        }),
      )
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const columns = useMemo<ColumnDef<UserSummary>[]>(
    () => [
      {
        id: 'user',
        accessorFn: (row) => `${row.display_name} ${row.email ?? ''}`,
        meta: { label: t('users.columns.user') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('users.columns.user')} />,
        cell: ({ row }) => {
          const u = row.original
          return (
            <div className="flex items-center gap-2.5">
              <Avatar className="size-7">
                <AvatarImage src={u.avatar_url ?? undefined} alt={u.display_name} referrerPolicy="no-referrer" />
                <AvatarFallback className="text-[10px]">
                  {u.display_name.slice(0, 2).toUpperCase()}
                </AvatarFallback>
              </Avatar>
              <div className="min-w-0">
                <div className="truncate font-medium">{u.display_name}</div>
                <div className="text-muted-foreground truncate text-xs">{u.email ?? '—'}</div>
              </div>
            </div>
          )
        },
      },
      {
        accessorKey: 'proxy_username',
        meta: { label: t('users.columns.proxyUsername') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('users.columns.proxyUsername')} />
        ),
        cell: ({ row }) => <span className="font-mono text-xs">{row.original.proxy_username}</span>,
      },
      {
        accessorKey: 'role',
        meta: { label: t('users.columns.role') },
        header: ({ column }) => <DataTableColumnHeader column={column} title={t('users.columns.role')} />,
        filterFn: (row, id, value: string[]) => value.includes(String(row.getValue(id))),
        cell: ({ row }) => {
          const u = row.original
          return (
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge variant={u.role === 'admin' ? 'default' : 'secondary'}>
                {u.role === 'admin' ? t('users.roles.admin') : t('users.roles.user')}
              </Badge>
              {u.account_type === 'relay' && (
                <Badge variant="outline">{t('users.accountTypes.relay')}</Badge>
              )}
            </div>
          )
        },
      },
      {
        accessorKey: 'account_type',
        meta: { label: t('users.columns.accountType') },
        filterFn: (row, id, value: string[]) => value.includes(String(row.getValue(id))),
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('users.columns.accountType')} />
        ),
        cell: ({ row }) => (
          <Badge variant="outline">
            {row.original.account_type === 'relay' ? t('users.accountTypes.relay') : t('users.accountTypes.user')}
          </Badge>
        ),
      },
      {
        id: 'proxy_access',
        accessorFn: (row) => (row.proxy_enabled ? 'enabled' : 'pending'),
        meta: { label: t('users.columns.proxyAccess') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('users.columns.proxyAccess')} />
        ),
        filterFn: (row, id, value: string[]) => value.includes(String(row.getValue(id))),
        cell: ({ row }) => {
          const enabled = row.original.proxy_enabled
          return (
            <Badge variant={enabled ? 'success' : 'warning'}>
              {enabled ? t('users.accessStates.enabled') : t('users.accessStates.pending')}
            </Badge>
          )
        },
      },
      {
        accessorKey: 'granted_node_count',
        meta: { label: t('users.columns.grantedNodes') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('users.columns.grantedNodes')} />
        ),
        cell: ({ row }) => <span className="font-mono text-xs">{row.original.granted_node_count}</span>,
      },
      {
        id: 'month_traffic',
        accessorFn: (row) => row.month_uplink_bytes + row.month_downlink_bytes,
        meta: { label: t('users.columns.monthTraffic') },
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title={t('users.columns.monthTraffic')} />
        ),
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
          const u = row.original
          return (
            <div className="flex justify-end gap-1.5">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  void openGrantsModal(u.id)
                }}
              >
                <KeyRound className="size-3.5" />
                <span>{u.proxy_enabled ? t('users.actions.grants') : t('users.actions.approve')}</span>
              </Button>
              <Button
                variant="ghost"
                size="icon"
                title={u.proxy_enabled ? t('users.actions.revokeProxy') : t('users.actions.enableProxy')}
                aria-label={u.proxy_enabled ? t('users.actions.revokeProxy') : t('users.actions.enableProxy')}
                onClick={() => {
                  toggleProxyMutation.mutate({ id: u.id, proxy_enabled: !u.proxy_enabled })
                }}
              >
                {u.proxy_enabled ? (
                  <Ban className="size-4 text-amber-600 dark:text-amber-400" />
                ) : (
                  <CheckCircle2 className="size-4 text-emerald-600 dark:text-emerald-400" />
                )}
              </Button>
              <Button
                variant="ghost"
                size="icon"
                title={t('users.actions.kickConnections')}
                aria-label={t('users.actions.kickConnections')}
                onClick={() => {
                  setKickTarget(u)
                }}
              >
                <UserX className="text-destructive size-4" />
              </Button>
            </div>
          )
        },
      },
    ],
    [t, toggleProxyMutation],
  )

  const filters = useMemo<ReadonlyArray<DataTableFilterConfig>>(
    () => [
      {
        columnId: 'proxy_access',
        title: t('users.columns.proxyAccess'),
        options: [
          { label: t('users.accessStates.enabled'), value: 'enabled' },
          { label: t('users.accessStates.pending'), value: 'pending' },
        ],
      },
      {
        columnId: 'role',
        title: t('users.columns.role'),
        options: [
          { label: t('users.roles.admin'), value: 'admin' },
          { label: t('users.roles.user'), value: 'user' },
        ],
      },
      {
        columnId: 'account_type',
        title: t('users.columns.accountType'),
        options: [
          { label: t('users.accountTypes.user'), value: 'user' },
          { label: t('users.accountTypes.relay'), value: 'relay' },
        ],
      },
    ],
    [t],
  )

  const editingUser = users.find((u) => u.id === grantUserId) ?? null

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{t('users.title')}</h1>
          <p className="text-muted-foreground text-sm">{t('users.subtitle')}</p>
        </div>
        <Button
          onClick={() => {
            setCreatedUser(null)
            setDisplayName('')
            setAccountType('relay')
            setProxyUsername('')
            setProxyUuid('')
            setProxyPassword('')
            setCreateNodeIds([])
            setCreateOpen(true)
          }}
        >
          <Plus className="size-4" />
          <span>{t('users.createRelayBtn')}</span>
        </Button>
      </div>

      <DataTable
        columns={columns}
        data={users}
        isLoading={isLoading}
        searchPlaceholder={t('users.searchPlaceholder')}
        filters={filters}
      />

      {/* Node Grants / Approval Dialog */}
      <Dialog
        open={grantUserId !== null}
        onOpenChange={(o) => {
          if (!o) {
            setGrantUserId(null)
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldCheck className="size-4" />
              <span>
                {editingUser?.proxy_enabled ? t('users.grantsDialog.editTitle') : t('users.grantsDialog.approveTitle')}
              </span>
            </DialogTitle>
            <DialogDescription>
              {editingUser?.display_name} ({editingUser?.proxy_username})
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <div className="text-muted-foreground text-xs font-medium uppercase">
                {t('users.grantsDialog.directNodes')}
              </div>
              <div className="max-h-36 space-y-2 overflow-y-auto rounded-md border p-2">
                {nodes.length === 0 ? (
                  <div className="text-muted-foreground py-2 text-center text-xs">
                    {t('users.grantsDialog.noNodes')}
                  </div>
                ) : (
                  nodes.map((n) => (
                    <label key={n.id} className="flex cursor-pointer items-center gap-2 text-sm">
                      <Checkbox
                        checked={nodeIds.includes(n.id)}
                        onCheckedChange={(checked) => {
                          setNodeIds((prev) =>
                            checked ? [...prev, n.id] : prev.filter((id) => id !== n.id),
                          )
                        }}
                      />
                      <span>{n.name}</span>
                    </label>
                  ))
                )}
              </div>
            </div>

            <div className="space-y-2">
              <div className="text-muted-foreground text-xs font-medium uppercase">
                {t('users.grantsDialog.nodeGroups')}
              </div>
              <div className="max-h-36 space-y-2 overflow-y-auto rounded-md border p-2">
                {groups.length === 0 ? (
                  <div className="text-muted-foreground py-2 text-center text-xs">
                    {t('users.grantsDialog.noGroups')}
                  </div>
                ) : (
                  groups.map((g) => (
                    <label key={g.id} className="flex cursor-pointer items-center gap-2 text-sm">
                      <Checkbox
                        checked={groupIds.includes(g.id)}
                        onCheckedChange={(checked) => {
                          setGroupIds((prev) =>
                            checked ? [...prev, g.id] : prev.filter((id) => id !== g.id),
                          )
                        }}
                      />
                      <span>{g.name}</span>
                    </label>
                  ))
                )}
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setGrantUserId(null)
              }}
            >
              {t('common.cancel')}
            </Button>
            {editingUser && !editingUser.proxy_enabled ? (
              <Button
                disabled={approveMutation.isPending}
                onClick={() => {
                  approveMutation.mutate(editingUser.id)
                }}
              >
                {t('users.grantsDialog.approveBtn')}
              </Button>
            ) : (
              editingUser && (
                <Button
                  disabled={saveGrantsMutation.isPending}
                  onClick={() => {
                    saveGrantsMutation.mutate(editingUser.id)
                  }}
                >
                  {t('users.grantsDialog.saveBtn')}
                </Button>
              )
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Create Relay / Bridge User Dialog */}
      <Dialog
        open={createOpen}
        onOpenChange={(o) => {
          setCreateOpen(o)
          if (!o) {
            setCreatedUser(null)
          }
        }}
      >
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t('users.createDialog.title')}</DialogTitle>
            <DialogDescription>{t('users.createDialog.desc')}</DialogDescription>
          </DialogHeader>

          {createdUser ? (
            <div className="space-y-3 rounded-md border p-3 text-sm">
              <div className="font-medium text-emerald-600 dark:text-emerald-400">
                {t('users.createDialog.createdBadge', { type: createdUser.account_type })}
              </div>
              <div className="space-y-1 font-mono text-xs">
                <div>
                  {t('users.createDialog.usernameLabel')}: {createdUser.proxy_username}
                </div>
                <div>
                  {t('users.createDialog.uuidLabel')}: {createdUser.proxy_uuid}
                </div>
                <div>
                  {t('users.createDialog.passwordLabel')}: {createdUser.proxy_password}
                </div>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  void navigator.clipboard.writeText(
                    `username=${createdUser.proxy_username}\nuuid=${createdUser.proxy_uuid}\npassword=${createdUser.proxy_password}`,
                  )
                  toast.success(t('users.createDialog.copiedCredentials'))
                }}
              >
                <Copy className="size-3.5" />
                <span>{t('users.createDialog.copyCredentials')}</span>
              </Button>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="grid gap-3 md:grid-cols-2">
                <div className="space-y-1">
                  <Label>{t('users.createDialog.displayName')}</Label>
                  <Input
                    value={displayName}
                    onChange={(e) => {
                      setDisplayName(e.target.value)
                    }}
                    placeholder="cn-sha-02 -> jp-hnd-01 relay"
                  />
                </div>
                <div className="space-y-1">
                  <Label>{t('users.createDialog.accountType')}</Label>
                  <Select
                    value={accountType}
                    onValueChange={(v) => {
                      setAccountType(v as 'relay' | 'user')
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="relay">{t('users.createDialog.typeRelay')}</SelectItem>
                      <SelectItem value="user">{t('users.createDialog.typeUser')}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="grid gap-3 md:grid-cols-2">
                <div className="space-y-1">
                  <Label>{t('users.createDialog.proxyUsernameOpt')}</Label>
                  <Input
                    value={proxyUsername}
                    onChange={(e) => {
                      setProxyUsername(e.target.value)
                    }}
                    placeholder="relay_cn_sha_02"
                  />
                </div>
                <div className="space-y-1">
                  <Label>{t('users.createDialog.proxyUuidOpt')}</Label>
                  <Input
                    value={proxyUuid}
                    onChange={(e) => {
                      setProxyUuid(e.target.value)
                    }}
                    placeholder={t('users.createDialog.proxyUuidPlaceholder')}
                  />
                </div>
              </div>

              <div className="space-y-1">
                <Label>{t('users.createDialog.proxyPasswordOpt')}</Label>
                <Input
                  value={proxyPassword}
                  onChange={(e) => {
                    setProxyPassword(e.target.value)
                  }}
                  placeholder={t('users.createDialog.proxyPasswordPlaceholder')}
                />
              </div>

              <div className="space-y-1.5">
                <Label>{t('users.createDialog.authorizedNodes')}</Label>
                <div className="max-h-36 space-y-2 overflow-y-auto rounded-md border p-2">
                  {nodes.length === 0 ? (
                    <div className="text-muted-foreground py-2 text-center text-xs">
                      {t('users.grantsDialog.noNodes')}
                    </div>
                  ) : (
                    nodes.map((n) => (
                      <label key={n.id} className="flex cursor-pointer items-center gap-2 text-sm">
                        <Checkbox
                          checked={createNodeIds.includes(n.id)}
                          onCheckedChange={(checked) => {
                            setCreateNodeIds((prev) =>
                              checked ? [...prev, n.id] : prev.filter((id) => id !== n.id),
                            )
                          }}
                        />
                        <span>{n.name}</span>
                      </label>
                    ))
                  )}
                </div>
              </div>
            </div>
          )}

          <DialogFooter>
            {createdUser ? (
              <Button
                onClick={() => {
                  setCreateOpen(false)
                }}
              >
                {t('common.done')}
              </Button>
            ) : (
              <>
                <Button
                  variant="outline"
                  onClick={() => {
                    setCreateOpen(false)
                  }}
                >
                  {t('common.cancel')}
                </Button>
                <Button
                  disabled={!displayName.trim() || createUserMutation.isPending}
                  onClick={() => {
                    createUserMutation.mutate()
                  }}
                >
                  {t('users.createDialog.createBtn')}
                </Button>
              </>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Kick Connections Confirm */}
      <ConfirmDialog
        open={kickTarget !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setKickTarget(null)
        }}
        title={t('users.kickConfirmTitle', { name: kickTarget?.display_name ?? '' })}
        desc={t('users.kickConfirmDesc')}
        confirmText={t('users.kickAction')}
        destructive
        isLoading={kickMutation.isPending}
        handleConfirm={() => {
          if (kickTarget) {
            kickMutation.mutate(kickTarget.id)
          }
        }}
      />
    </div>
  )
}
