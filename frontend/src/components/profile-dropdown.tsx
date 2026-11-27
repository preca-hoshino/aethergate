import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { Activity, LogOut, Settings } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { AuthUser } from '@/types/api'
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { SignOutDialog } from '@/components/sign-out-dialog'

interface ProfileDropdownProps {
  user: AuthUser
}

export function ProfileDropdown({ user }: ProfileDropdownProps) {
  const { t } = useTranslation()
  const [signOutOpen, setSignOutOpen] = useState(false)
  const initials = user.display_name.slice(0, 2).toUpperCase() || 'AG'

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" className="relative size-8 rounded-full p-0">
            <Avatar className="size-8">
              <AvatarImage src={user.avatar_url ?? undefined} alt={user.display_name} referrerPolicy="no-referrer" />
              <AvatarFallback>{initials}</AvatarFallback>
            </Avatar>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent className="w-56" align="end" forceMount>
          <DropdownMenuLabel className="font-normal">
            <div className="flex flex-col gap-1">
              <p className="truncate text-sm leading-none font-medium">{user.display_name}</p>
              <p className="text-muted-foreground truncate text-xs leading-none">
                {user.email ?? user.proxy_username}
              </p>
            </div>
          </DropdownMenuLabel>
          <DropdownMenuSeparator />
          <DropdownMenuGroup>
            <DropdownMenuItem asChild>
              <Link to="/portal">
                <Activity className="size-4" />
                <span>{t('nav.userPortal')}</span>
              </Link>
            </DropdownMenuItem>
            {user.role === 'admin' && (
              <DropdownMenuItem asChild>
                <Link to="/settings">
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

      <SignOutDialog open={signOutOpen} onOpenChange={setSignOutOpen} />
    </>
  )
}
