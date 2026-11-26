import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { AuthProvidersResponse } from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

export function SettingsPage() {
  const { data: providers } = useQuery({
    queryKey: ['auth-providers'],
    queryFn: () => api.get<AuthProvidersResponse>('/api/v1/auth/providers'),
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Settings</h1>
        <p className="text-muted-foreground text-sm">Control plane configuration summary.</p>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>OIDC / SSO Provider</CardTitle>
            <CardDescription>Configured via `config.server.toml` / environment variables.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Status</span>
              <Badge variant={providers?.oidc_enabled ? 'success' : 'secondary'}>
                {providers?.oidc_enabled ? 'Enabled' : 'Disabled'}
              </Badge>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Provider</span>
              <span className="font-medium">{providers?.provider_name ?? '—'}</span>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Telemetry &amp; Egress Policy</CardTitle>
            <CardDescription>Optimized for Aliyun ECS outbound egress savings.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Idle Cadence</span>
              <span className="font-mono">Eco (30s)</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Live Watcher Cadence</span>
              <span className="font-mono">Realtime (5s)</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Traffic Reset Policy</span>
              <span className="font-mono">1st of Calendar Month</span>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
