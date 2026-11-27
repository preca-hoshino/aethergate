import type * as React from 'react'
import { SearchIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/stores/use-app-store'
import { Button } from '@/components/ui/button'

export function Search({ className, ...props }: React.ComponentProps<'button'>) {
  const { t } = useTranslation()
  const setCommandOpen = useAppStore((s) => s.setCommandOpen)

  return (
    <Button
      variant="outline"
      className={cn(
        'bg-muted/25 text-muted-foreground hover:bg-accent group relative h-8 w-full flex-1 justify-start rounded-md text-sm font-normal shadow-none sm:w-44 sm:pe-12 md:flex-none lg:w-56 xl:w-64',
        className,
      )}
      aria-keyshortcuts="Meta+K Control+K"
      onClick={() => {
        setCommandOpen(true)
      }}
      {...props}
    >
      <SearchIcon aria-hidden="true" className="size-3.5 shrink-0" />
      <span className="truncate">{t('command.triggerPlaceholder')}</span>
      <kbd className="bg-muted group-hover:bg-accent pointer-events-none absolute right-1.5 top-1/2 hidden h-5 -translate-y-1/2 items-center gap-0.5 rounded border px-1.5 font-mono text-[10px] font-medium select-none sm:flex">
        <span>⌘</span>K
      </kbd>
    </Button>
  )
}
