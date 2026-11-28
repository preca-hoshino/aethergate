import { useCallback, useEffect } from 'react'
import { useNavigate } from '@tanstack/react-router'
import {
  Activity,
  BarChart3,
  Boxes,
  FileText,
  Languages,
  LayoutDashboard,
  Moon,
  Server,
  Settings,
  Sun,
  Users,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { setAppLocale } from '@/i18n'
import { useAppStore } from '@/stores/use-app-store'
import type { AuthUser } from '@/types/api'
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from '@/components/ui/command'
import { ScrollArea } from '@/components/ui/scroll-area'

interface CommandMenuProps {
  role?: AuthUser['role']
}

export function CommandMenu({ role = 'admin' }: CommandMenuProps) {
  const { t } = useTranslation()
  const open = useAppStore((s) => s.commandOpen)
  const setOpen = useAppStore((s) => s.setCommandOpen)
  const setTheme = useAppStore((s) => s.setTheme)
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

  const runCommand = useCallback(
    (action: () => void) => {
      setOpen(false)
      action()
    },
    [setOpen],
  )

  const navCommands =
    role === 'admin'
      ? [
          { label: t('command.items.overview'), to: '/', icon: LayoutDashboard },
          { label: t('command.items.nodes'), to: '/nodes', icon: Server },
          { label: t('command.items.nodeGroups'), to: '/node-groups', icon: Boxes },
          { label: t('command.items.users'), to: '/users', icon: Users },
          { label: t('command.items.analytics'), to: '/analytics', icon: BarChart3 },
          { label: t('command.items.auditLogs'), to: '/audit-logs', icon: FileText },
          { label: t('command.items.settings'), to: '/settings', icon: Settings },
          { label: t('command.items.portal'), to: '/portal', icon: Activity },
        ]
      : [{ label: t('command.items.portal'), to: '/portal', icon: Activity }]

  return (
    <CommandDialog
      open={open}
      onOpenChange={setOpen}
      title={t('command.dialogTitle')}
      description={t('command.dialogDescription')}
    >
      <CommandInput placeholder={t('command.inputPlaceholder')} />
      <CommandList>
        <ScrollArea className="h-72 pe-1">
          <CommandEmpty>{t('command.empty')}</CommandEmpty>
          <CommandGroup heading={t('command.navigationGroup')}>
            {navCommands.map((item) => {
              const Icon = item.icon
              return (
                <CommandItem
                  key={item.to}
                  value={`${item.label} ${item.to}`}
                  onSelect={() => {
                    runCommand(() => {
                      void navigate({ to: item.to })
                    })
                  }}
                >
                  <Icon className="opacity-70" />
                  <span>{item.label}</span>
                </CommandItem>
              )
            })}
          </CommandGroup>

          <CommandSeparator />

          <CommandGroup heading={t('command.themeGroup')}>
            <CommandItem
              onSelect={() => {
                runCommand(() => {
                  setTheme('light')
                })
              }}
            >
              <Sun />
              <span>{t('theme.light')}</span>
            </CommandItem>
            <CommandItem
              onSelect={() => {
                runCommand(() => {
                  setTheme('dark')
                })
              }}
            >
              <Moon />
              <span>{t('theme.dark')}</span>
            </CommandItem>
          </CommandGroup>

          <CommandSeparator />

          <CommandGroup heading={t('command.languageGroup')}>
            <CommandItem
              onSelect={() => {
                runCommand(() => {
                  void setAppLocale('zh-CN')
                })
              }}
            >
              <Languages />
              <span>{t('language.zhCN')}</span>
            </CommandItem>
            <CommandItem
              onSelect={() => {
                runCommand(() => {
                  void setAppLocale('en')
                })
              }}
            >
              <Languages />
              <span>{t('language.en')}</span>
            </CommandItem>
          </CommandGroup>
        </ScrollArea>
      </CommandList>
    </CommandDialog>
  )
}
