import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { Activity, ChevronsUpDown, LogOut, Settings } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { AuthUser } from '@/types/api'
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { SidebarMenu, SidebarMenuButton, SidebarMenuItem, useSidebar } from '@/components/ui/sidebar'
import { SignOutDialog } from '@/components/sign-out-dialog'

interface NavUserProps {
  user: AuthUser
}

export function NavUser({ user }: NavUserProps) {
  const { t } = useTranslation()
  const { isMobile, setOpenMobile } = useSidebar()
  const [signOutOpen, setSignOutOpen] = useState(false)
  const initials = user.display_name.slice(0, 2).toUpperCase() || 'AG'

  return (
    <>
      <SidebarMenu>
        <SidebarMenuItem>
          <DropdownMenu modal={false}>
            <DropdownMenuTrigger asChild>
              <SidebarMenuButton
                size="lg"
                className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
              >
                <Avatar className="size-8 rounded-lg">
                  <AvatarImage
                    src={user.avatar_url ?? undefined}
                    alt={user.display_name}
                    referrerPolicy="no-referrer"
                  />
                  <AvatarFallback className="rounded-lg">{initials}</AvatarFallback>
                </Avatar>
                <div className="grid flex-1 text-start text-sm leading-tight">
                  <span className="truncate font-semibold">{user.display_name}</span>
                  <span className="text-muted-foreground truncate text-xs">
                    {user.email ?? user.proxy_username}
                  </span>
                </div>
                <ChevronsUpDown className="ms-auto size-4 opacity-60" />
              </SidebarMenuButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              className="w-(--radix-dropdown-menu-trigger-width) min-w-56 rounded-lg"
              side={isMobile ? 'bottom' : 'right'}
              align="end"
              sideOffset={4}
            >
              <DropdownMenuLabel className="p-0 font-normal">
                <div className="flex items-center gap-2 px-1.5 py-1.5 text-start text-sm">
                  <Avatar className="size-8 rounded-lg">
                    <AvatarImage
                      src={user.avatar_url ?? undefined}
                      alt={user.display_name}
                      referrerPolicy="no-referrer"
                    />
                    <AvatarFallback className="rounded-lg">{initials}</AvatarFallback>
                  </Avatar>
                  <div className="grid flex-1 text-start text-sm leading-tight">
                    <span className="truncate font-semibold">{user.display_name}</span>
                    <span className="text-muted-foreground truncate text-xs">
                      {user.email ?? user.proxy_username}
                    </span>
                  </div>
                </div>
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuGroup>
                <DropdownMenuItem asChild>
                  <Link
                    to="/portal"
                    onClick={() => {
                      setOpenMobile(false)
                    }}
                  >
                    <Activity className="size-4" />
                    <span>{t('nav.userPortal')}</span>
                  </Link>
                </DropdownMenuItem>
                {user.role === 'admin' && (
                  <DropdownMenuItem asChild>
                    <Link
                      to="/settings"
                      onClick={() => {
                        setOpenMobile(false)
                      }}
                    >
                      <Settings className="size-4" />
                      <span>{t('nav.settings')}</span>
                    </Link>
                  </DropdownMenuItem>
                )}
              </DropdownMenuGroup>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                onClick={() => {
                  setSignOutOpen(true)
                }}
              >
                <LogOut className="size-4" />
                <span>{t('nav.signOut')}</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </SidebarMenuItem>
      </SidebarMenu>

      <SignOutDialog open={signOutOpen} onOpenChange={setSignOutOpen} />
    </>
  )
}
