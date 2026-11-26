import { useQuery } from '@tanstack/react-query'
import { ArrowRight, Lock, Shield, Sparkles } from 'lucide-react'
import { api } from '@/lib/api-client'
import type { AuthProvidersResponse } from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

export function SignInPage() {
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
    <div className="container relative grid min-h-screen flex-col items-center justify-center lg:max-w-none lg:grid-cols-2 lg:px-0">
      {/* Left Split-Screen Brand Panel */}
      <div className="bg-muted relative hidden h-full flex-col justify-between p-10 text-white lg:flex dark:border-r">
        <div className="absolute inset-0 bg-zinc-950" />
        <div className="relative z-20 flex items-center gap-2.5 text-lg font-semibold">
          <div className="bg-primary text-primary-foreground flex size-8 items-center justify-center rounded-lg">
            <Shield className="size-4" />
          </div>
          <span>Æthergate Control Plane</span>
        </div>

        <div className="relative z-20 space-y-4">
          <Badge variant="outline" className="border-zinc-700 text-zinc-300">
            <Sparkles className="size-3" />
            sing-box 1.12+ · 4-Protocol Orchestration
          </Badge>
          <h2 className="text-3xl font-semibold tracking-tight">
            Zero-trust multi-node proxy management &amp; 3D monthly telemetry.
          </h2>
          <p className="max-w-md text-sm text-zinc-400">
            Manage VLESS Reality, Hysteria 2, TUIC v5, and AnyTLS nodes with adaptive Eco/Realtime
            gRPC telemetry, Hysteria 2-inspired Open API, and OIDC single sign-on.
          </p>
        </div>

        <div className="relative z-20 text-xs text-zinc-500">
          Secured by OIDC Authorization Code + PKCE &amp; HttpOnly BFF Sessions
        </div>
      </div>

      {/* Right Sign-In Column */}
      <div className="flex items-center justify-center p-6 lg:p-10">
        <Card className="w-full max-w-md border-0 shadow-none sm:border sm:shadow-sm">
          <CardHeader className="space-y-2 text-center">
            <div className="bg-primary/10 text-primary mx-auto flex size-11 items-center justify-center rounded-xl lg:hidden">
              <Shield className="size-5" />
            </div>
            <CardTitle className="text-2xl font-semibold tracking-tight">Sign in to Æthergate</CardTitle>
            <CardDescription>
              Authenticate with your organization&apos;s OIDC identity provider. New accounts require
              administrator approval before proxy access is enabled.
            </CardDescription>
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
                {isLoading ? 'Checking OIDC provider...' : `Continue with ${providerName}`}
              </span>
              <ArrowRight className="size-4" />
            </Button>

            {!isLoading && !oidcEnabled && (
              <p className="text-destructive text-center text-xs">
                OIDC is currently disabled on this server. Configure `[oidc]` in `config.server.toml`.
              </p>
            )}

            <p className="text-muted-foreground text-center text-xs">
              Local password login is disabled by policy. All sessions use HttpOnly SameSite cookies.
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
