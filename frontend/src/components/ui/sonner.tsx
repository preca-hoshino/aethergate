import type * as React from 'react'
import { Toaster as Sonner, type ToasterProps } from 'sonner'
import { useAppStore } from '@/stores/use-app-store'

export function Toaster({ ...props }: ToasterProps) {
  const theme = useAppStore((s) => s.theme)

  return (
    <Sonner
      theme={theme}
      className="toaster group [&_div[data-content]]:w-full"
      style={
        {
          '--normal-bg': 'var(--popover)',
          '--normal-text': 'var(--popover-foreground)',
          '--normal-border': 'var(--border)',
        } as React.CSSProperties
      }
      {...props}
    />
  )
}
