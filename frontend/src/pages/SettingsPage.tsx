import { useEffect, useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Save } from 'lucide-react'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import type { AuthProvidersResponse } from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Label } from '@/components/ui/label'

interface MihomoTemplateDto {
  header_and_dns_yaml: string
  rules_yaml: string
}

export function SettingsPage() {
  const { data: providers } = useQuery({
    queryKey: ['auth-providers'],
    queryFn: () => api.get<AuthProvidersResponse>('/api/v1/auth/providers'),
  })

  const { data: template } = useQuery({
    queryKey: ['mihomo-template'],
    queryFn: () => api.get<MihomoTemplateDto>('/api/v1/settings/mihomo-template'),
  })

  const [headerDns, setHeaderDns] = useState('')
  const [rulesYaml, setRulesYaml] = useState('')

  useEffect(() => {
    if (template) {
      setHeaderDns(template.header_and_dns_yaml)
      setRulesYaml(template.rules_yaml)
    }
  }, [template])

  const saveMutation = useMutation({
    mutationFn: () =>
      api.put<undefined>('/api/v1/settings/mihomo-template', {
        header_and_dns_yaml: headerDns,
        rules_yaml: rulesYaml,
      }),
    onSuccess: () => {
      toast.success('Mihomo subscription template saved')
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Settings</h1>
        <p className="text-muted-foreground text-sm">
          Control plane configuration &amp; Mihomo / Clash Meta subscription template.
        </p>
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

      <Card>
        <CardHeader>
          <CardTitle>Mihomo / Clash Meta Subscription Template (`?format=mihomo`)</CardTitle>
          <CardDescription>
            Customize the global `dns:` header and `rules:` footer. `proxies:` and 2-level `proxy-groups:`
            (PROXY &rarr; per-node group &rarr; 4 protocols) are generated automatically per user.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-1.5">
            <Label>Header &amp; DNS YAML</Label>
            <textarea
              rows={12}
              value={headerDns}
              onChange={(e) => {
                setHeaderDns(e.target.value)
              }}
              className="border-input bg-background w-full rounded-md border p-3 font-mono text-xs outline-none focus-visible:ring-2"
            />
          </div>
          <div className="space-y-1.5">
            <Label>Rules YAML</Label>
            <textarea
              rows={10}
              value={rulesYaml}
              onChange={(e) => {
                setRulesYaml(e.target.value)
              }}
              className="border-input bg-background w-full rounded-md border p-3 font-mono text-xs outline-none focus-visible:ring-2"
            />
          </div>
          <Button
            disabled={saveMutation.isPending}
            onClick={() => {
              saveMutation.mutate()
            }}
          >
            <Save className="size-4" />
            Save Mihomo Template
          </Button>
        </CardContent>
      </Card>
    </div>
  )
}
