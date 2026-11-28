import { Moon, Sun } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { runWithThemeTransition } from '@/lib/view-transition'
import { useAppStore } from '@/stores/use-app-store'
import { Button } from '@/components/ui/button'

export function ThemeSwitch() {
  const { t } = useTranslation()
  const theme = useAppStore((s) => s.theme)
  const setTheme = useAppStore((s) => s.setTheme)

  return (
    <Button
      variant="ghost"
      size="icon"
      className="rounded-full"
      aria-label={t('theme.toggleAria')}
      title={t('theme.toggleAria')}
      onClick={(e) => {
        const nextTheme = theme === 'dark' ? 'light' : 'dark'
        void runWithThemeTransition(() => {
          setTheme(nextTheme)
        }, e)
      }}
    >
      <Sun className="size-[1.15rem] scale-100 rotate-0 transition-all dark:scale-0 dark:-rotate-90" />
      <Moon className="absolute size-[1.15rem] scale-0 rotate-90 transition-all dark:scale-100 dark:rotate-0" />
      <span className="sr-only">{t('theme.toggleAria')}</span>
    </Button>
  )
}
