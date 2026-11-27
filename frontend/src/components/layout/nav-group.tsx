import { Link, useRouterState } from '@tanstack/react-router'
import type { LucideIcon } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import {
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from '@/components/ui/sidebar'

export interface NavItemConfig {
  title: string
  to: string
  icon: LucideIcon
  badge?: string
}

interface NavGroupProps {
  title: string
  items: ReadonlyArray<NavItemConfig>
}

export function NavGroup({ title, items }: NavGroupProps) {
  const { setOpenMobile } = useSidebar()
  const pathname = useRouterState({ select: (s) => s.location.pathname })

  return (
    <SidebarGroup>
      <SidebarGroupLabel>{title}</SidebarGroupLabel>
      <SidebarMenu>
        {items.map((item) => {
          const Icon = item.icon
          const active = item.to === '/' ? pathname === '/' : pathname.startsWith(item.to)
          return (
            <SidebarMenuItem key={item.to}>
              <SidebarMenuButton asChild isActive={active} tooltip={item.title}>
                <Link
                  to={item.to}
                  onClick={() => {
                    setOpenMobile(false)
                  }}
                >
                  <Icon />
                  <span>{item.title}</span>
                  {item.badge && (
                    <Badge variant="secondary" className="ms-auto rounded-full px-1.5 py-0 text-[10px]">
                      {item.badge}
                    </Badge>
                  )}
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          )
        })}
      </SidebarMenu>
    </SidebarGroup>
  )
}
