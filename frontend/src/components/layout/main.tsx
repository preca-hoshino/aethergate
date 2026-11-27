import type * as React from 'react'
import { cn } from '@/lib/utils'

type MainProps = React.HTMLAttributes<HTMLElement> & {
  fixed?: boolean
  fluid?: boolean
}

export function Main({ fixed = false, fluid = false, className, ...props }: MainProps) {
  return (
    <main
      data-layout={fixed ? 'fixed' : 'auto'}
      className={cn(
        'flex-1 px-4 py-6 md:px-6',
        fixed && 'flex grow flex-col overflow-hidden',
        !fluid && '@7xl/content:mx-auto @7xl/content:w-full @7xl/content:max-w-7xl',
        className,
      )}
      {...props}
    />
  )
}
