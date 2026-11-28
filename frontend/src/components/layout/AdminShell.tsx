import type { ReactNode } from 'react'
import { Radio } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import { useLiveMetricsStore } from '@/stores/use-live-metrics-store'
import type { AuthUser } from '@/types/api'
import { Badge } from '@/components/ui/badge'
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

export function AdminShell({ user, children }: AdminShellProps) {
  const { t } = useTranslation()

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
