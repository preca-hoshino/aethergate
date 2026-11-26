import { useEffect } from 'react'
import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  useNavigate,
  useRouterState,
} from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { api, setCsrfToken } from '@/lib/api-client'
import { wsManager } from '@/lib/ws-manager'
import type { AuthUser } from '@/types/api'
import { AdminShell } from '@/components/layout/AdminShell'
import { AnalyticsPage } from '@/pages/AnalyticsPage'
import { AuditLogsPage } from '@/pages/AuditLogsPage'
import { DashboardPage } from '@/pages/DashboardPage'
import { NodeDetailPage } from '@/pages/NodeDetailPage'
import { NodeGroupsPage } from '@/pages/NodeGroupsPage'
import { NodesPage } from '@/pages/NodesPage'
import { PortalPage } from '@/pages/PortalPage'
import { SettingsPage } from '@/pages/SettingsPage'
import { SignInPage } from '@/pages/SignInPage'
import { UsersPage } from '@/pages/UsersPage'

function RootComponent() {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const navigate = useNavigate()
  const isSignIn = pathname === '/sign-in'

  const { data: user, isLoading, isError } = useQuery({
    queryKey: ['auth-me'],
    queryFn: () => api.get<AuthUser>('/api/v1/auth/me'),
    retry: false,
    enabled: !isSignIn,
  })

  useEffect(() => {
    if (user?.csrf_token) {
      setCsrfToken(user.csrf_token)
    }
    if (user?.role === 'admin') {
      wsManager.start()
      return () => {
        wsManager.stop()
      }
    }
    return undefined
  }, [user])

  useEffect(() => {
    if (!isSignIn && !isLoading && (isError || !user)) {
      void navigate({ to: '/sign-in' })
    } else if (user && user.role !== 'admin' && pathname !== '/portal' && !isSignIn) {
      void navigate({ to: '/portal' })
    }
  }, [isSignIn, isLoading, isError, user, pathname, navigate])

  if (isSignIn) {
    return <Outlet />
  }

  if (isLoading || !user) {
    return (
      <div className="bg-background text-muted-foreground flex min-h-screen items-center justify-center text-sm">
        Verifying session...
      </div>
    )
  }

  return (
    <AdminShell user={user}>
      <Outlet />
    </AdminShell>
  )
}

const rootRoute = createRootRoute({ component: RootComponent })

const signInRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/sign-in',
  component: SignInPage,
})

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: DashboardPage,
})

const nodesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/nodes',
  component: NodesPage,
})

const nodeDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/nodes/$nodeId',
  component: NodeDetailPage,
})

const nodeGroupsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/node-groups',
  component: NodeGroupsPage,
})

const usersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/users',
  component: UsersPage,
})

const analyticsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/analytics',
  component: AnalyticsPage,
})

const auditLogsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/audit-logs',
  component: AuditLogsPage,
})

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: SettingsPage,
})

const portalRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/portal',
  component: PortalPage,
})

const routeTree = rootRoute.addChildren([
  signInRoute,
  dashboardRoute,
  nodesRoute,
  nodeDetailRoute,
  nodeGroupsRoute,
  usersRoute,
  analyticsRoute,
  auditLogsRoute,
  settingsRoute,
  portalRoute,
])

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
