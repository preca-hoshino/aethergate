import { useEffect } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link, Outlet, useNavigate, useRouterState } from '@tanstack/react-router'
import { Loader2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { api, setCsrfToken } from '@/lib/api-client'
import { wsManager } from '@/lib/ws-manager'
import type { AuthUser } from '@/types/api'
import { AdminShell } from '@/components/layout/AdminShell'
import { Button } from '@/components/ui/button'

export function RootLayout() {
  const { t } = useTranslation()
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const navigate = useNavigate()
  const isSignIn = pathname === '/sign-in'

  const { data: user, isLoading, isError } = useQuery({
    queryKey: ['auth-me'],
    queryFn: () => api.get<AuthUser>('/api/v1/auth/me'),
    retry: false,
    enabled: !isSignIn,
  })

  useEffect(() => {
    if (user?.csrf_token) {
      setCsrfToken(user.csrf_token)
    }
    if (user?.role === 'admin') {
      wsManager.start()
      return () => {
        wsManager.stop()
      }
    }
    return undefined
  }, [user])

  useEffect(() => {
    if (!isSignIn && !isLoading && (isError || !user)) {
      void navigate({ to: '/sign-in' })
    } else if (user && user.role !== 'admin' && pathname !== '/portal' && !isSignIn) {
      void navigate({ to: '/portal' })
    }
  }, [isSignIn, isLoading, isError, user, pathname, navigate])

  if (isSignIn) {
    return <Outlet />
  }

  if (isLoading || !user) {
    return (
      <div className="bg-background text-muted-foreground flex min-h-svh items-center justify-center gap-2 text-sm">
        <Loader2 className="size-4 animate-spin" />
        <span>{t('common.verifyingSession')}</span>
      </div>
    )
  }

  return (
    <AdminShell user={user}>
      <Outlet />
    </AdminShell>
  )
}

export function NotFoundView() {
  const { t } = useTranslation()
  return (
    <div className="flex min-h-[60vh] flex-col items-center justify-center gap-3 text-center">
      <div className="text-6xl font-bold tracking-tight">{t('notFound.code')}</div>
      <h1 className="text-xl font-semibold">{t('notFound.title')}</h1>
      <p className="text-muted-foreground max-w-md text-sm">{t('notFound.description')}</p>
      <Button asChild className="mt-2">
        <Link to="/">{t('notFound.backHome')}</Link>
      </Button>
    </div>
  )
}
