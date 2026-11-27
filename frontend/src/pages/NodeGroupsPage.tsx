import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Boxes, Plus, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import type { NodeGroup, NodeSummary } from '@/types/api'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'

export function NodeGroupsPage() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [selectedNodes, setSelectedNodes] = useState<string[]>([])
  const [deleteTarget, setDeleteTarget] = useState<NodeGroup | null>(null)

  const { data: groups = [], isLoading: groupsLoading } = useQuery({
    queryKey: ['node-groups'],
    queryFn: () => api.get<NodeGroup[]>('/api/v1/node-groups'),
  })

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes-for-groups'],
    queryFn: () => api.get<NodeSummary[]>('/api/v1/nodes'),
  })

  const createMutation = useMutation({
    mutationFn: () =>
      api.post('/api/v1/node-groups', {
        name,
        description,
        node_ids: selectedNodes,
      }),
    onSuccess: () => {
      setOpen(false)
      setName('')
      setDescription('')
      setSelectedNodes([])
      void queryClient.invalidateQueries({ queryKey: ['node-groups'] })
      toast.success(t('nodeGroups.createdToast'))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete<undefined>(`/api/v1/node-groups/${id}`),
    onSuccess: () => {
      setDeleteTarget(null)
      void queryClient.invalidateQueries({ queryKey: ['node-groups'] })
      toast.success(t('nodeGroups.deletedToast'))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{t('nodeGroups.title')}</h1>
          <p className="text-muted-foreground text-sm">{t('nodeGroups.subtitle')}</p>
        </div>

        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button>
              <Plus className="size-4" />
              <span>{t('nodeGroups.newGroup')}</span>
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{t('nodeGroups.dialogTitle')}</DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <div className="space-y-1.5">
                <Label>{t('nodeGroups.form.name')}</Label>
                <Input
                  placeholder={t('nodeGroups.form.namePlaceholder')}
                  value={name}
                  onChange={(e) => {
                    setName(e.target.value)
                  }}
                />
              </div>
              <div className="space-y-1.5">
                <Label>{t('nodeGroups.form.description')}</Label>
                <Input
                  placeholder={t('nodeGroups.form.descriptionPlaceholder')}
                  value={description}
                  onChange={(e) => {
                    setDescription(e.target.value)
                  }}
                />
              </div>
              <div className="space-y-2">
                <Label>{t('nodeGroups.form.memberNodes')}</Label>
                <div className="max-h-40 space-y-2 overflow-y-auto rounded-md border p-2">
                  {nodes.length === 0 ? (
                    <div className="text-muted-foreground py-2 text-center text-xs">
                      {t('nodeGroups.form.noNodesAvailable')}
                    </div>
                  ) : (
                    nodes.map((n) => (
                      <label key={n.id} className="flex cursor-pointer items-center gap-2 text-sm">
                        <Checkbox
                          checked={selectedNodes.includes(n.id)}
                          onCheckedChange={(checked) => {
                            setSelectedNodes((prev) =>
                              checked ? [...prev, n.id] : prev.filter((id) => id !== n.id),
                            )
                          }}
                        />
                        <span>{n.name}</span>
                        <span className="text-muted-foreground text-xs">({n.region})</span>
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
                  setOpen(false)
                }}
              >
                {t('common.cancel')}
              </Button>
              <Button
                disabled={!name.trim() || createMutation.isPending}
                onClick={() => {
                  createMutation.mutate()
                }}
              >
                {t('nodeGroups.createBtn')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      {groupsLoading ? (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          <Skeleton className="h-32 rounded-xl" />
          <Skeleton className="h-32 rounded-xl" />
          <Skeleton className="h-32 rounded-xl" />
        </div>
      ) : groups.length > 0 ? (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {groups.map((g) => (
            <Card key={g.id}>
              <CardHeader className="flex flex-row items-start justify-between">
                <div className="min-w-0">
                  <CardTitle className="flex items-center gap-2 text-base">
                    <Boxes className="size-4 shrink-0" />
                    <span className="truncate">{g.name}</span>
                  </CardTitle>
                  <CardDescription>{g.description || t('common.noDescription')}</CardDescription>
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t('common.delete')}
                  onClick={() => {
                    setDeleteTarget(g)
                  }}
                >
                  <Trash2 className="text-destructive size-4" />
                </Button>
              </CardHeader>
              <CardContent className="text-muted-foreground text-xs">
                {t('nodeGroups.memberCount', { count: g.member_count })}
              </CardContent>
            </Card>
          ))}
        </div>
      ) : (
        <Card>
          <CardContent className="text-muted-foreground py-12 text-center text-sm">
            {t('nodeGroups.empty')}
          </CardContent>
        </Card>
      )}

      <ConfirmDialog
        open={deleteTarget !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setDeleteTarget(null)
        }}
        title={t('nodeGroups.deleteConfirmTitle', { name: deleteTarget?.name ?? '' })}
        desc={t('nodeGroups.deleteConfirmDesc')}
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
