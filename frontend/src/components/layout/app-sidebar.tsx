import { Link } from '@tanstack/react-router'
import {
  Activity,
  BarChart3,
  Boxes,
  FileText,
  LayoutDashboard,
  Server,
  Settings,
  Shield,
  Users,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '@/stores/use-app-store'
import type { AuthUser } from '@/types/api'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
} from '@/components/ui/sidebar'
import { NavGroup, type NavItemConfig } from '@/components/layout/nav-group'
import { NavUser } from '@/components/layout/nav-user'

interface AppSidebarProps {
  user: AuthUser
}

export function AppSidebar({ user }: AppSidebarProps) {
  const { t } = useTranslation()
  const variant = useAppStore((s) => s.sidebarVariant)
  const collapsible = useAppStore((s) => s.sidebarCollapsible)

  const isAdmin = user.role === 'admin'

  const generalItems: ReadonlyArray<NavItemConfig> = [
    { title: t('nav.overview'), to: '/', icon: LayoutDashboard },
    { title: t('nav.nodes'), to: '/nodes', icon: Server },
    { title: t('nav.nodeGroups'), to: '/node-groups', icon: Boxes },
    { title: t('nav.users'), to: '/users', icon: Users },
    { title: t('nav.analytics'), to: '/analytics', icon: BarChart3 },
  ]

  const managementItems: ReadonlyArray<NavItemConfig> = [
    { title: t('nav.myPortal'), to: '/portal', icon: Activity },
    { title: t('nav.auditLogs'), to: '/audit-logs', icon: FileText },
    { title: t('nav.settings'), to: '/settings', icon: Settings },
  ]

  const userPortalItems: ReadonlyArray<NavItemConfig> = [
    { title: t('nav.myPortal'), to: '/portal', icon: Activity },
  ]

  return (
    <Sidebar variant={variant} collapsible={collapsible}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <Link to={isAdmin ? '/' : '/portal'}>
                <div className="bg-sidebar-primary text-sidebar-primary-foreground flex aspect-square size-8 items-center justify-center rounded-lg">
                  <Shield className="size-4" />
                </div>
                <div className="grid flex-1 text-start text-sm leading-tight">
                  <span className="truncate font-semibold">{t('brand.name')}</span>
                  <span className="text-muted-foreground truncate text-xs">{t('brand.subtitle')}</span>
                </div>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <SidebarContent>
        {isAdmin ? (
          <>
            <NavGroup title={t('nav.generalGroup')} items={generalItems} />
            <NavGroup title={t('nav.otherGroup')} items={managementItems} />
          </>
        ) : (
          <NavGroup title={t('nav.generalGroup')} items={userPortalItems} />
        )}
      </SidebarContent>

      <SidebarFooter>
        <NavUser user={user} />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  )
}
