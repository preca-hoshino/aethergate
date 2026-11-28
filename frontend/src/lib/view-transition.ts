import { flushSync } from 'react-dom'

/**
 * Run a theme state update with a circular View Transitions API reveal
 * radiating from the pointer click coordinates.
 */
export async function runWithThemeTransition(
  update: () => void,
  event?: Pick<MouseEvent, 'clientX' | 'clientY'>,
): Promise<void> {
  const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

  if (typeof document.startViewTransition !== 'function' || !event || prefersReducedMotion) {
    update()
    return
  }

  const x = event.clientX
  const y = event.clientY
  const endRadius = Math.hypot(
    Math.max(x, window.innerWidth - x),
    Math.max(y, window.innerHeight - y),
  )

  const transition = document.startViewTransition(() => {
    flushSync(() => {
      update()
    })
  })

  await transition.ready

  document.documentElement.animate(
    {
      clipPath: [
        `circle(0px at ${x}px ${y}px)`,
        `circle(${endRadius}px at ${x}px ${y}px)`,
      ],
    },
    {
      duration: 300,
      easing: 'ease-out',
      pseudoElement: '::view-transition-new(root)',
    },
  )
}
