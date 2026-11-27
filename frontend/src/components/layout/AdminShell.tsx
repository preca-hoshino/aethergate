import type { ReactNode } from 'react'
import { Calendar, Check, Radio } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/stores/use-app-store'
import { useLiveMetricsStore } from '@/stores/use-live-metrics-store'
import type { AuthUser } from '@/types/api'
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
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { LanguageSwitch } from '@/components/language-switch'
import { AppSidebar } from '@/components/layout/app-sidebar'
import { CommandMenu } from '@/components/layout/CommandMenu'
import { Header } from '@/components/layout/header'
import { Main } from '@/components/layout/main'
import { ProfileDropdown } from '@/components/profile-dropdown'
import { Search } from '@/components/search'
import { ThemeSwitch } from '@/components/theme-switch'

interface AdminShellProps {
  user: AuthUser
  children: ReactNode
}

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
  const { t } = useTranslation()
  const selectedMonth = useAppStore((s) => s.selectedMonth)
  const setSelectedMonth = useAppStore((s) => s.setSelectedMonth)

  const wsConnected = useLiveMetricsStore((s) => s.connected)
  const cadenceMode = useLiveMetricsStore((s) => s.cadenceMode)
  const isAdmin = user.role === 'admin'

  const cadenceLabel =
    cadenceMode === 'realtime' ? t('common.cadenceRealtime') : t('common.cadenceEco')

  return (
    <SidebarProvider>
      <CommandMenu role={user.role} />
      <AppSidebar user={user} />
      <SidebarInset
        className={cn(
          '@container/content',
          'has-data-[layout=fixed]:h-svh',
          'peer-data-[variant=inset]:has-data-[layout=fixed]:h-[calc(100svh-(var(--spacing)*4))]',
        )}
      >
        <Header fixed>
          {isAdmin && <Search />}

          <div className="ms-auto flex items-center gap-2">
            {isAdmin && (
              <Badge variant={wsConnected ? 'success' : 'secondary'} className="hidden sm:inline-flex">
                <Radio className="size-3" />
                <span>
                  {wsConnected ? `${t('common.live')} (${cadenceLabel})` : t('common.offline')}
                </span>
              </Badge>
            )}

            <DropdownMenu modal={false}>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" className="h-8 gap-1.5">
                  <Calendar className="size-3.5" />
                  <span className="font-mono text-xs">{selectedMonth}</span>
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>{t('common.calendarMonth')}</DropdownMenuLabel>
                <DropdownMenuSeparator />
                {recentMonths().map((m) => (
                  <DropdownMenuItem
                    key={m}
                    onClick={() => {
                      setSelectedMonth(m)
                    }}
                  >
                    <span className="font-mono">{m}</span>
                    <Check className={cn('ms-auto size-4', selectedMonth !== m && 'hidden')} />
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>

            <LanguageSwitch />
            <ThemeSwitch />
            <ProfileDropdown user={user} />
          </div>
        </Header>

        <Main>{children}</Main>
      </SidebarInset>
    </SidebarProvider>
  )
}
