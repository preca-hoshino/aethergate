import { useQuery } from '@tanstack/react-query'
import { ArrowRight, Lock, Shield, Sparkles } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { api } from '@/lib/api-client'
import type { AuthProvidersResponse } from '@/types/api'
import { LanguageSwitch } from '@/components/language-switch'
import { ThemeSwitch } from '@/components/theme-switch'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

export function SignInPage() {
  const { t } = useTranslation()
  const { data: providers, isLoading } = useQuery({
    queryKey: ['auth-providers'],
    queryFn: () => api.get<AuthProvidersResponse>('/api/v1/auth/providers'),
  })

  const oidcEnabled = providers?.oidc_enabled ?? true
  const providerName = providers?.provider_name ?? 'Logto / OIDC'
  const loginUrl = providers?.login_url ?? '/api/v1/auth/oidc/login'

  const handleOidcJump = () => {
    window.location.href = loginUrl
  }

  return (
    <div className="relative grid min-h-svh flex-col items-center justify-center lg:grid-cols-2">
      {/* Left Split-Screen Brand Panel */}
      <div className="bg-muted relative hidden h-full flex-col justify-between p-10 text-white lg:flex dark:border-r">
        <div className="absolute inset-0 bg-zinc-950" />
        <div className="relative z-20 flex items-center gap-2.5 text-lg font-semibold">
          <div className="bg-primary text-primary-foreground flex size-8 items-center justify-center rounded-lg">
            <Shield className="size-4" />
          </div>
          <span>{t('auth.brandTitle')}</span>
        </div>

        <div className="relative z-20 space-y-4">
          <Badge variant="outline" className="border-zinc-700 text-zinc-300">
            <Sparkles className="size-3" />
            <span>{t('auth.badge')}</span>
          </Badge>
          <h2 className="text-3xl font-semibold tracking-tight">{t('auth.heroHeadline')}</h2>
          <p className="max-w-md text-sm text-zinc-400">{t('auth.heroDescription')}</p>
        </div>

        <div className="relative z-20 text-xs text-zinc-500">{t('auth.heroFooter')}</div>
      </div>

      {/* Right Sign-In Column */}
      <div className="relative flex h-full items-center justify-center p-6 lg:p-10">
        <div className="absolute top-4 right-4 flex items-center gap-1">
          <LanguageSwitch />
          <ThemeSwitch />
        </div>

        <Card className="w-full max-w-md border-0 shadow-none sm:border sm:shadow-sm">
          <CardHeader className="space-y-2 text-center">
            <div className="bg-primary/10 text-primary mx-auto flex size-11 items-center justify-center rounded-xl lg:hidden">
              <Shield className="size-5" />
            </div>
            <CardTitle className="text-2xl font-semibold tracking-tight">{t('auth.signInTitle')}</CardTitle>
            <CardDescription>{t('auth.signInDescription')}</CardDescription>
          </CardHeader>

          <CardContent className="space-y-4">
            <Button
              size="lg"
              className="w-full justify-between"
              disabled={isLoading || !oidcEnabled}
              onClick={handleOidcJump}
            >
              <span className="flex items-center gap-2">
                <Lock className="size-4" />
                {isLoading
                  ? t('auth.checkingProvider')
                  : t('auth.continueWith', { provider: providerName })}
              </span>
              <ArrowRight className="size-4" />
            </Button>

            {!isLoading && !oidcEnabled && (
              <p className="text-destructive text-center text-xs">{t('auth.oidcDisabledHint')}</p>
            )}

            <p className="text-muted-foreground text-center text-xs">{t('auth.localLoginDisabled')}</p>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
