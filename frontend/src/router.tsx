import { createRootRoute, createRoute, createRouter } from '@tanstack/react-router'
import { NotFoundView, RootLayout } from '@/components/layout/RootLayout'
import { AuditLogsPage } from '@/pages/AuditLogsPage'
import { DashboardPage } from '@/pages/DashboardPage'
import { DataBoardPage } from '@/pages/DataBoardPage'
import { NodeDetailPage } from '@/pages/NodeDetailPage'
import { NodeGroupsPage } from '@/pages/NodeGroupsPage'
import { NodesPage } from '@/pages/NodesPage'
import { PortalPage } from '@/pages/PortalPage'
import { SettingsPage } from '@/pages/SettingsPage'
import { SignInPage } from '@/pages/SignInPage'
import { UsersPage } from '@/pages/UsersPage'

const rootRoute = createRootRoute({ component: RootLayout, notFoundComponent: NotFoundView })

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

const dataBoardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/data-board',
  component: DataBoardPage,
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
  dataBoardRoute,
  auditLogsRoute,
  settingsRoute,
  portalRoute,
])

export const router = createRouter({
  routeTree,
  defaultNotFoundComponent: NotFoundView,
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
