import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Boxes, Plus, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import type { NodeGroup, NodeSummary } from '@/types/api'
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

export function NodeGroupsPage() {
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [selectedNodes, setSelectedNodes] = useState<string[]>([])

  const { data: groups = [] } = useQuery({
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
      toast.success('Node group created')
    },
    onError: (err: Error) => toast.error(err.message),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete<undefined>(`/api/v1/node-groups/${id}`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['node-groups'] })
      toast.success('Node group deleted')
    },
  })

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Node Groups</h1>
          <p className="text-muted-foreground text-sm">
            Bundle edge nodes into logical groups for batch user authorization.
          </p>
        </div>

        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button>
              <Plus className="size-4" />
              New Group
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Create Node Group</DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <div className="space-y-1.5">
                <Label>Group Name</Label>
                <Input placeholder="Asia-Pacific Premium" value={name} onChange={(e) => { setName(e.target.value); }} />
              </div>
              <div className="space-y-1.5">
                <Label>Description</Label>
                <Input
                  placeholder="Low-latency JP/SG/HK nodes"
                  value={description}
                  onChange={(e) => { setDescription(e.target.value); }}
                />
              </div>
              <div className="space-y-2">
                <Label>Member Nodes</Label>
                <div className="max-h-40 space-y-2 overflow-y-auto rounded-md border p-2">
                  {nodes.map((n) => (
                    <label key={n.id} className="flex cursor-pointer items-center gap-2 text-sm">
                      <Checkbox
                        checked={selectedNodes.includes(n.id)}
                        onCheckedChange={(checked) => { setSelectedNodes((prev) =>
                            checked ? [...prev, n.id] : prev.filter((id) => id !== n.id),
                          ); }
                        }
                      />
                      <span>{n.name}</span>
                      <span className="text-muted-foreground text-xs">({n.region})</span>
                    </label>
                  ))}
                </div>
              </div>
            </div>
            <DialogFooter>
              <Button
                disabled={!name || createMutation.isPending}
                onClick={() => {
                  createMutation.mutate()
                }}
              >
                Create Group
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {groups.map((g) => (
          <Card key={g.id}>
            <CardHeader className="flex flex-row items-start justify-between">
              <div>
                <CardTitle className="flex items-center gap-2 text-base">
                  <Boxes className="size-4" />
                  {g.name}
                </CardTitle>
                <CardDescription>{g.description || 'No description'}</CardDescription>
              </div>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => {
                  deleteMutation.mutate(g.id)
                }}
              >
                <Trash2 className="text-destructive size-4" />
              </Button>
            </CardHeader>
            <CardContent className="text-muted-foreground text-xs">
              {g.member_count} member node(s)
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  )
}
