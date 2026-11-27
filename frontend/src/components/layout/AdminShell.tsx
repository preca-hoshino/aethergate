import { useState, type ReactNode } from 'react'
import { Link, useNavigate, useRouterState } from '@tanstack/react-router'
import {
  Activity,
  BarChart3,
  Boxes,
  Calendar,
  ChevronsUpDown,
  FileText,
  LayoutDashboard,
  LogOut,
  Menu,
  Moon,
  PanelLeft,
  Radio,
  Search,
  Server,
  Settings,
  Shield,
  Sun,
  Users,
} from 'lucide-react'
import { api } from '@/lib/api-client'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/stores/use-app-store'
import { useLiveMetricsStore } from '@/stores/use-live-metrics-store'
import type { AuthUser } from '@/types/api'
import { Avatar, AvatarFallback } from '@/components/ui/avatar'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { CommandMenu } from './CommandMenu'

interface AdminShellProps {
  user: AuthUser
  children: ReactNode
}

const GENERAL_NAV = [
  { label: 'Overview', to: '/', icon: LayoutDashboard },
  { label: 'Nodes', to: '/nodes', icon: Server },
  { label: 'Node Groups', to: '/node-groups', icon: Boxes },
  { label: 'Users', to: '/users', icon: Users },
  { label: 'Analytics', to: '/analytics', icon: BarChart3 },
] as const

const OTHER_NAV = [
  { label: 'My Portal', to: '/portal', icon: Activity },
  { label: 'Audit Logs', to: '/audit-logs', icon: FileText },
  { label: 'Settings', to: '/settings', icon: Settings },
] as const

function recentMonths(count = 6): string[] {
  const out: string[] = []
  const now = new Date()
  for (let i = 0; i < count; i += 1) {
    const d = new Date(now.getFullYear(), now.getMonth() - i, 1)
    out.push(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`)
  }
  return out
}

export function AdminShell({ user, children }: AdminShellProps) {
  const [mobileOpen, setMobileOpen] = useState(false)
  const collapsed = useAppStore((s) => s.sidebarCollapsed)
  const toggleSidebar = useAppStore((s) => s.toggleSidebar)
  const setCommandOpen = useAppStore((s) => s.setCommandOpen)
  const theme = useAppStore((s) => s.theme)
  const setTheme = useAppStore((s) => s.setTheme)
  const selectedMonth = useAppStore((s) => s.selectedMonth)
  const setSelectedMonth = useAppStore((s) => s.setSelectedMonth)

  const wsConnected = useLiveMetricsStore((s) => s.connected)
  const cadenceMode = useLiveMetricsStore((s) => s.cadenceMode)
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const navigate = useNavigate()

  const handleLogout = async () => {
    try {
      const res = await api.post<{ end_session_url: string | null }>('/api/v1/auth/logout')
      if (res.end_session_url) {
        window.location.href = res.end_session_url
        return
      }
    } catch {
      // Fallback to local sign-in route
    }
    void navigate({ to: '/sign-in' })
  }

  const initials = user.display_name.slice(0, 2).toUpperCase()
  const isAdmin = user.role === 'admin'

  const renderNavItems = (items: ReadonlyArray<{ label: string; to: string; icon: typeof LayoutDashboard }>) =>
    items.map((item) => {
      const Icon = item.icon
      const active = item.to === '/' ? pathname === '/' : pathname.startsWith(item.to)
      return (
        <Link
          key={item.to}
          to={item.to}
          onClick={() => {
            setMobileOpen(false)
          }}
          className={cn(
            'flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm font-medium transition-colors',
            active
              ? 'bg-sidebar-accent text-sidebar-accent-foreground'
              : 'text-sidebar-foreground/75 hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground',
          )}
        >
          <Icon className="size-4 shrink-0" />
          {!collapsed && <span className="truncate">{item.label}</span>}
        </Link>
      )
    })

  return (
    <div className="bg-background text-foreground flex min-h-screen w-full">
      <CommandMenu />

      {/* Desktop Sidebar */}
      <aside
        className={cn(
          'bg-sidebar text-sidebar-foreground border-sidebar-border hidden shrink-0 flex-col border-r transition-all duration-200 md:flex',
          collapsed ? 'w-16' : 'w-64',
        )}
      >
        <div className="border-sidebar-border flex h-14 items-center gap-2.5 border-b px-3">
          <div className="bg-primary text-primary-foreground flex size-8 shrink-0 items-center justify-center rounded-lg">
            <Shield className="size-4" />
          </div>
          {!collapsed && (
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-semibold">Æthergate</div>
              <div className="text-muted-foreground truncate text-xs">sing-box Control Plane</div>
            </div>
          )}
        </div>

        <nav className="flex-1 space-y-5 overflow-y-auto p-3">
          {isAdmin ? (
            <>
              <div className="space-y-1">
                {!collapsed && (
                  <div className="text-muted-foreground px-2.5 pb-1 text-xs font-medium">General</div>
                )}
                {renderNavItems(GENERAL_NAV)}
              </div>
              <div className="space-y-1">
                {!collapsed && (
                  <div className="text-muted-foreground px-2.5 pb-1 text-xs font-medium">Other</div>
                )}
                {renderNavItems(OTHER_NAV)}
              </div>
            </>
          ) : (
            <div className="space-y-1">
              {renderNavItems([{ label: 'My Portal', to: '/portal', icon: LayoutDashboard }])}
            </div>
          )}
        </nav>

        <div className="border-sidebar-border border-t p-3">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className="hover:bg-sidebar-accent flex w-full items-center gap-2.5 rounded-md p-1.5 text-left text-sm"
              >
                <Avatar className="size-8">
                  <AvatarFallback>{initials}</AvatarFallback>
                </Avatar>
                {!collapsed && (
                  <>
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-medium">{user.display_name}</div>
                      <div className="text-muted-foreground truncate text-xs">{user.email ?? user.proxy_username}</div>
                    </div>
                    <ChevronsUpDown className="size-4 opacity-60" />
                  </>
                )}
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              <DropdownMenuLabel>{user.display_name}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={() => {
                  void navigate({ to: '/portal' })
                }}
              >
                User Portal
              </DropdownMenuItem>
              {isAdmin && (
                <DropdownMenuItem
                  onClick={() => {
                    void navigate({ to: '/settings' })
                  }}
                >
                  Settings
                </DropdownMenuItem>
              )}
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={() => {
                  void handleLogout()
                }}
                className="text-destructive"
              >
                <LogOut className="size-4" />
                Sign out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </aside>

      {/* Mobile Drawer */}
      {mobileOpen && (
        <div className="fixed inset-0 z-50 flex md:hidden">
          <div
            className="fixed inset-0 bg-black/50"
            onClick={() => {
              setMobileOpen(false)
            }}
          />
          <aside className="bg-sidebar text-sidebar-foreground relative z-10 flex w-64 flex-col border-r p-3">
            <div className="mb-4 flex items-center gap-2 px-2">
              <Shield className="size-5 text-primary" />
              <span className="font-semibold">Æthergate</span>
            </div>
            <div className="space-y-1">
              {isAdmin
                ? renderNavItems([...GENERAL_NAV, ...OTHER_NAV])
                : renderNavItems([{ label: 'My Portal', to: '/portal', icon: LayoutDashboard }])}
            </div>
          </aside>
        </div>
      )}

      {/* Main Content */}
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="bg-background/95 sticky top-0 z-30 flex h-14 items-center gap-3 border-b px-4 backdrop-blur-sm">
          <Button
            variant="ghost"
            size="icon"
            className="md:hidden"
            onClick={() => {
              setMobileOpen(true)
            }}
          >
            <Menu className="size-5" />
          </Button>
          <Button variant="ghost" size="icon" className="hidden md:inline-flex" onClick={toggleSidebar}>
            <PanelLeft className="size-4" />
          </Button>

          {isAdmin && (
            <Button
              variant="outline"
              size="sm"
              className="text-muted-foreground hidden w-60 justify-between md:inline-flex"
              onClick={() => {
                setCommandOpen(true)
              }}
            >
              <span className="flex items-center gap-2">
                <Search className="size-3.5" />
                Search...
              </span>
              <kbd className="bg-muted rounded px-1.5 py-0.5 text-[10px] font-medium">⌘K</kbd>
            </Button>
          )}

          <div className="ml-auto flex items-center gap-2">
            {isAdmin && (
              <Badge variant={wsConnected ? 'success' : 'secondary'} className="hidden sm:inline-flex">
                <Radio className="size-3" />
                {wsConnected ? `Live (${cadenceMode})` : 'Offline'}
              </Badge>
            )}

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" className="gap-1.5">
                  <Calendar className="size-3.5" />
                  <span>{selectedMonth}</span>
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>Calendar Month</DropdownMenuLabel>
                <DropdownMenuSeparator />
                {recentMonths().map((m) => (
                  <DropdownMenuItem
                    key={m}
                    onClick={() => {
                      setSelectedMonth(m)
                    }}
                  >
                    {m}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>

            <Button
              variant="ghost"
              size="icon"
              onClick={() => {
                setTheme(theme === 'dark' ? 'light' : 'dark')
              }}
              aria-label="Toggle theme"
            >
              {theme === 'dark' ? <Sun className="size-4" /> : <Moon className="size-4" />}
            </Button>
          </div>
        </header>

        <main className="flex-1 p-4 md:p-6">{children}</main>
      </div>
    </div>
  )
}
