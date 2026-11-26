import { useEffect } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { Command } from 'cmdk'
import {
  Activity,
  BarChart3,
  Boxes,
  FileText,
  LayoutDashboard,
  Server,
  Settings,
  Users,
} from 'lucide-react'
import { useAppStore } from '@/stores/use-app-store'
import { Dialog, DialogContent } from '@/components/ui/dialog'

const ADMIN_COMMANDS = [
  { label: 'Dashboard Overview', to: '/', icon: LayoutDashboard },
  { label: 'Nodes & 4-Protocol Builder', to: '/nodes', icon: Server },
  { label: 'Node Groups', to: '/node-groups', icon: Boxes },
  { label: 'Users & Approvals', to: '/users', icon: Users },
  { label: '3D Monthly Analytics', to: '/analytics', icon: BarChart3 },
  { label: 'Audit Logs', to: '/audit-logs', icon: FileText },
  { label: 'System Settings', to: '/settings', icon: Settings },
  { label: 'User Portal View', to: '/portal', icon: Activity },
] as const

export function CommandMenu() {
  const open = useAppStore((s) => s.commandOpen)
  const setOpen = useAppStore((s) => s.setCommandOpen)
  const navigate = useNavigate()

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setOpen(!open)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [open, setOpen])

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="overflow-hidden p-0 sm:max-w-lg">
        <Command className="[&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium">
          <div className="flex items-center border-b px-3">
            <Command.Input
              placeholder="Jump to page or action..."
              className="placeholder:text-muted-foreground flex h-11 w-full bg-transparent py-3 text-sm outline-none"
            />
          </div>
          <Command.List className="max-h-72 overflow-y-auto p-2">
            <Command.Empty className="py-6 text-center text-sm text-muted-foreground">
              No matching command.
            </Command.Empty>
            <Command.Group heading="Navigation">
              {ADMIN_COMMANDS.map((item) => {
                const Icon = item.icon
                return (
                  <Command.Item
                    key={item.to}
                    value={item.label}
                    onSelect={() => {
                      setOpen(false)
                      void navigate({ to: item.to })
                    }}
                    className="aria-selected:bg-accent aria-selected:text-accent-foreground flex cursor-pointer items-center gap-2 rounded-sm px-2 py-2 text-sm"
                  >
                    <Icon className="size-4 opacity-70" />
                    <span>{item.label}</span>
                  </Command.Item>
                )
              })}
            </Command.Group>
          </Command.List>
        </Command>
      </DialogContent>
    </Dialog>
  )
}
