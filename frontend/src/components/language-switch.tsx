import { Check, Languages } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { getActiveLocale, setAppLocale, type AppLocale } from '@/i18n'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

const LOCALES: ReadonlyArray<{ code: AppLocale; labelKey: 'language.zhCN' | 'language.en' }> = [
  { code: 'zh-CN', labelKey: 'language.zhCN' },
  { code: 'en', labelKey: 'language.en' },
]

export function LanguageSwitch() {
  const { t, i18n } = useTranslation()
  const activeLocale = i18n.language.toLowerCase().startsWith('zh') ? 'zh-CN' : getActiveLocale()

  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" className="rounded-full" aria-label={t('language.toggleAria')}>
          <Languages className="size-[1.15rem]" />
          <span className="sr-only">{t('language.toggleAria')}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {LOCALES.map((item) => (
          <DropdownMenuItem
            key={item.code}
            onClick={() => {
              void setAppLocale(item.code)
            }}
          >
            <span>{t(item.labelKey)}</span>
            <Check className={cn('ms-auto size-4', activeLocale !== item.code && 'hidden')} />
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
