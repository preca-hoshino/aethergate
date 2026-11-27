import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Ban, CheckCircle2, Copy, KeyRound, Plus, ShieldCheck, UserX } from 'lucide-react'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import { formatBytes } from '@/lib/formatters'
import { useAppStore } from '@/stores/use-app-store'
import type { NodeGroup, NodeSummary, UserDetail, UserSummary } from '@/types/api'
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'

interface CreatedUserResult {
  id: string
  proxy_username: string
  proxy_uuid: string
  proxy_password: string
  account_type: 'user' | 'relay'
}

export function UsersPage() {
  const month = useAppStore((s) => s.selectedMonth)
  const queryClient = useQueryClient()
  const [filter, setFilter] = useState<'all' | 'pending' | 'active'>('all')
  const [grantUserId, setGrantUserId] = useState<string | null>(null)
  const [nodeIds, setNodeIds] = useState<string[]>([])
  const [groupIds, setGroupIds] = useState<string[]>([])

  const [createOpen, setCreateOpen] = useState(false)
  const [displayName, setDisplayName] = useState('')
  const [accountType, setAccountType] = useState<'relay' | 'user'>('relay')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyUuid, setProxyUuid] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')
  const [createNodeIds, setCreateNodeIds] = useState<string[]>([])
  const [createdUser, setCreatedUser] = useState<CreatedUserResult | null>(null)

  const { data: users = [] } = useQuery({
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
      toast.success('User approved & node grants synced')
    },
    onError: (err: Error) => toast.error(err.message),
  })

  const saveGrantsMutation = useMutation({
    mutationFn: (id: string) => api.put(`/api/v1/users/${id}/grants`, { node_ids: nodeIds, group_ids: groupIds }),
    onSuccess: () => {
      setGrantUserId(null)
      void queryClient.invalidateQueries({ queryKey: ['users'] })
      toast.success('Node grants updated')
    },
  })

  const toggleProxyMutation = useMutation({
    mutationFn: ({ id, proxy_enabled }: { id: string; proxy_enabled: boolean }) =>
      api.patch(`/api/v1/users/${id}`, { proxy_enabled }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['users'] })
      toast.success('User proxy status updated')
    },
  })

  const kickMutation = useMutation({
    mutationFn: (id: string) => api.post(`/api/v1/users/${id}/kick`),
    onSuccess: () => toast.success('Kick command dispatched to authorized nodes'),
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
      toast.success(`Created ${res.account_type} account (${res.proxy_username})`)
    },
    onError: (err: Error) => toast.error(err.message),
  })

  const filtered = users.filter((u) => {
    if (filter === 'pending') return !u.proxy_enabled
    if (filter === 'active') return u.proxy_enabled
    return true
  })

  const editingUser = users.find((u) => u.id === grantUserId) ?? null

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Users &amp; Approvals</h1>
          <p className="text-muted-foreground text-sm">
            Approve OIDC self-registered accounts, manage relay service accounts, and monitor monthly traffic.
          </p>
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
          Create Relay / Bridge User
        </Button>
      </div>

      <Tabs value={filter} onValueChange={(v) => { setFilter(v as typeof filter); }}>
        <TabsList>
          <TabsTrigger value="all">All ({users.length})</TabsTrigger>
          <TabsTrigger value="pending">
            Pending Approval ({users.filter((u) => !u.proxy_enabled).length})
          </TabsTrigger>
          <TabsTrigger value="active">Active ({users.filter((u) => u.proxy_enabled).length})</TabsTrigger>
        </TabsList>
      </Tabs>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">User Directory</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>User</TableHead>
                <TableHead>Proxy Username</TableHead>
                <TableHead>Role</TableHead>
                <TableHead>Proxy Access</TableHead>
                <TableHead>Granted Nodes</TableHead>
                <TableHead>Month Traffic</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.map((u) => (
                <TableRow key={u.id}>
                  <TableCell>
                    <div className="flex items-center gap-2.5">
                      <Avatar className="size-7">
                        <AvatarImage
                          src={u.avatar_url ?? undefined}
                          alt={u.display_name}
                          referrerPolicy="no-referrer"
                        />
                        <AvatarFallback className="text-[10px]">
                          {u.display_name.slice(0, 2).toUpperCase()}
                        </AvatarFallback>
                      </Avatar>
                      <div className="min-w-0">
                        <div className="truncate font-medium">{u.display_name}</div>
                        <div className="text-muted-foreground truncate text-xs">{u.email ?? '—'}</div>
                      </div>
                    </div>
                  </TableCell>
                  <TableCell className="font-mono text-xs">{u.proxy_username}</TableCell>
                  <TableCell>
                    <div className="flex flex-wrap items-center gap-1.5">
                      <Badge variant={u.role === 'admin' ? 'default' : 'secondary'}>{u.role}</Badge>
                      {u.account_type === 'relay' && <Badge variant="outline">relay</Badge>}
                    </div>
                  </TableCell>
                  <TableCell>
                    <Badge variant={u.proxy_enabled ? 'success' : 'warning'}>
                      {u.proxy_enabled ? 'enabled' : 'pending'}
                    </Badge>
                  </TableCell>
                  <TableCell>{u.granted_node_count}</TableCell>
                  <TableCell className="font-mono text-xs">
                    {formatBytes(u.month_uplink_bytes + u.month_downlink_bytes)}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1.5">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => {
                          void openGrantsModal(u.id)
                        }}
                      >
                        <KeyRound className="size-3.5" />
                        {u.proxy_enabled ? 'Grants' : 'Approve'}
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        title={u.proxy_enabled ? 'Revoke proxy access' : 'Enable proxy access'}
                        onClick={() => {
                          toggleProxyMutation.mutate({ id: u.id, proxy_enabled: !u.proxy_enabled })
                        }}
                      >
                        {u.proxy_enabled ? (
                          <Ban className="text-amber-600 size-4" />
                        ) : (
                          <CheckCircle2 className="text-emerald-600 size-4" />
                        )}
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        title="Kick active connections"
                        onClick={() => {
                          kickMutation.mutate(u.id)
                        }}
                      >
                        <UserX className="text-destructive size-4" />
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

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
              {editingUser?.proxy_enabled ? 'Edit Node Grants' : 'Approve User & Assign Grants'}
            </DialogTitle>
            <DialogDescription>
              {editingUser?.display_name} ({editingUser?.proxy_username})
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <div className="text-xs font-medium uppercase text-muted-foreground">Direct Nodes</div>
              <div className="max-h-36 space-y-2 overflow-y-auto rounded-md border p-2">
                {nodes.map((n) => (
                  <label key={n.id} className="flex cursor-pointer items-center gap-2 text-sm">
                    <Checkbox
                      checked={nodeIds.includes(n.id)}
                      onCheckedChange={(checked) => { setNodeIds((prev) => (checked ? [...prev, n.id] : prev.filter((id) => id !== n.id))); }
                      }
                    />
                    <span>{n.name}</span>
                  </label>
                ))}
              </div>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-medium uppercase text-muted-foreground">Node Groups</div>
              <div className="max-h-36 space-y-2 overflow-y-auto rounded-md border p-2">
                {groups.map((g) => (
                  <label key={g.id} className="flex cursor-pointer items-center gap-2 text-sm">
                    <Checkbox
                      checked={groupIds.includes(g.id)}
                      onCheckedChange={(checked) => { setGroupIds((prev) => (checked ? [...prev, g.id] : prev.filter((id) => id !== g.id))); }
                      }
                    />
                    <span>{g.name}</span>
                  </label>
                ))}
              </div>
            </div>
          </div>

          <DialogFooter>
            {editingUser && !editingUser.proxy_enabled ? (
              <Button
                onClick={() => {
                  approveMutation.mutate(editingUser.id)
                }}
              >
                Approve &amp; Save Grants
              </Button>
            ) : (
              editingUser && (
                <Button
                  onClick={() => {
                    saveGrantsMutation.mutate(editingUser.id)
                  }}
                >
                  Save Grants
                </Button>
              )
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={createOpen}
        onOpenChange={(o) => {
          setCreateOpen(o)
        }}
      >
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>Create Relay Service Account / Bridge User</DialogTitle>
            <DialogDescription>
              Relay accounts exclude traffic from end-user billing rankings and return 403 on public subscription links.
            </DialogDescription>
          </DialogHeader>

          {createdUser ? (
            <div className="space-y-3 rounded-md border p-3 text-sm">
              <div className="font-medium text-emerald-600 dark:text-emerald-400">
                Account created ({createdUser.account_type})
              </div>
              <div className="space-y-1 font-mono text-xs">
                <div>Username: {createdUser.proxy_username}</div>
                <div>UUID: {createdUser.proxy_uuid}</div>
                <div>Password: {createdUser.proxy_password}</div>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  void navigator.clipboard.writeText(
                    `username=${createdUser.proxy_username}\nuuid=${createdUser.proxy_uuid}\npassword=${createdUser.proxy_password}`,
                  )
                  toast.success('Credentials copied')
                }}
              >
                <Copy className="size-3.5" />
                Copy Credentials
              </Button>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="grid gap-3 md:grid-cols-2">
                <div className="space-y-1">
                  <Label>Display Name</Label>
                  <Input
                    value={displayName}
                    onChange={(e) => {
                      setDisplayName(e.target.value)
                    }}
                    placeholder="cn-sha-02 -> jp-hnd-01 relay"
                  />
                </div>
                <div className="space-y-1">
                  <Label>Account Type</Label>
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
                      <SelectItem value="relay">relay (Service / Transit)</SelectItem>
                      <SelectItem value="user">user (Legacy Migration Bridge)</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="grid gap-3 md:grid-cols-2">
                <div className="space-y-1">
                  <Label>Proxy Username (optional)</Label>
                  <Input
                    value={proxyUsername}
                    onChange={(e) => {
                      setProxyUsername(e.target.value)
                    }}
                    placeholder="relay_cn_sha_02"
                  />
                </div>
                <div className="space-y-1">
                  <Label>Proxy UUID (optional)</Label>
                  <Input
                    value={proxyUuid}
                    onChange={(e) => {
                      setProxyUuid(e.target.value)
                    }}
                    placeholder="auto-generated if blank"
                  />
                </div>
              </div>

              <div className="space-y-1">
                <Label>Proxy Password (optional)</Label>
                <Input
                  value={proxyPassword}
                  onChange={(e) => {
                    setProxyPassword(e.target.value)
                  }}
                  placeholder="auto-generated 24-char token if blank"
                />
              </div>

              <div className="space-y-1.5">
                <Label>Authorized Nodes</Label>
                <div className="max-h-36 space-y-2 overflow-y-auto rounded-md border p-2">
                  {nodes.map((n) => (
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
                  ))}
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
                Done
              </Button>
            ) : (
              <Button
                disabled={!displayName.trim() || createUserMutation.isPending}
                onClick={() => {
                  createUserMutation.mutate()
                }}
              >
                Create Account
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
