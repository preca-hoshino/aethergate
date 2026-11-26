import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Clock, Copy, QrCode, ShieldCheck } from 'lucide-react'
import { QRCodeSVG } from 'qrcode.react'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import { formatBps, formatBytes } from '@/lib/formatters'
import { useAppStore } from '@/stores/use-app-store'
import type { PortalSubscriptionLink, PortalSummary } from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'

export function PortalPage() {
  const month = useAppStore((s) => s.selectedMonth)
  const [subFormat, setSubFormat] = useState<'mihomo' | 'singbox' | 'uri'>('mihomo')

  const { data: summary, isLoading } = useQuery({
    queryKey: ['portal-summary', month],
    queryFn: () => api.get<PortalSummary>(`/api/v1/portal/summary?month=${month}`),
  })

  const { data: links = [] } = useQuery({
    queryKey: ['portal-subscriptions'],
    queryFn: () => api.get<PortalSubscriptionLink[]>('/api/v1/portal/subscriptions'),
    enabled: Boolean(summary?.proxy_enabled),
  })

  if (isLoading || !summary) {
    return <div className="text-muted-foreground p-6 text-sm">Loading your portal...</div>
  }

  if (!summary.proxy_enabled) {
    return (
      <div className="mx-auto max-w-xl py-10">
        <Card>
          <CardHeader>
            <div className="bg-amber-500/15 text-amber-600 dark:text-amber-400 mb-2 flex size-10 items-center justify-center rounded-lg">
              <Clock className="size-5" />
            </div>
            <CardTitle>Pending Administrator Approval</CardTitle>
            <CardDescription>
              Your OIDC account ({summary.proxy_username}) is active, but proxy node access has not
              been enabled yet. Please contact an administrator to assign your node permissions.
            </CardDescription>
          </CardHeader>
        </Card>
      </div>
    )
  }

  const formattedSubUrl =
    subFormat === 'uri' ? summary.subscription_url : `${summary.subscription_url}?format=${subFormat}`

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">My Proxy Portal</h1>
        <p className="text-muted-foreground text-sm">
          Monthly usage ({month}) &amp; 4-protocol subscription endpoints for {summary.proxy_username}.
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Monthly Total</CardTitle>
          </CardHeader>
          <CardContent className="text-2xl font-bold">
            {formatBytes(summary.uplink_bytes + summary.downlink_bytes)}
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Uplink / Downlink</CardTitle>
          </CardHeader>
          <CardContent className="font-mono text-sm">
            ↑ {formatBytes(summary.uplink_bytes)} · ↓ {formatBytes(summary.downlink_bytes)}
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Peak Bandwidth</CardTitle>
          </CardHeader>
          <CardContent className="font-mono text-sm">
            ↓ {formatBps(summary.peak_downlink_bps)}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <CardTitle className="flex items-center gap-2 text-base">
              <QrCode className="size-4" />
              Universal Subscription URL
            </CardTitle>
            <CardDescription>
              Full Mihomo / Clash Meta YAML (2-level node &amp; protocol groups), sing-box JSON, or Base64 URIs.
            </CardDescription>
          </div>
          <Tabs
            value={subFormat}
            onValueChange={(v) => {
              setSubFormat(v as typeof subFormat)
            }}
          >
            <TabsList>
              <TabsTrigger value="mihomo">Mihomo / Clash</TabsTrigger>
              <TabsTrigger value="singbox">sing-box</TabsTrigger>
              <TabsTrigger value="uri">Base64 URI</TabsTrigger>
            </TabsList>
          </Tabs>
        </CardHeader>
        <CardContent className="flex flex-col items-start gap-6 sm:flex-row sm:items-center">
          <div className="rounded-lg border bg-white p-3">
            <QRCodeSVG value={formattedSubUrl} size={132} />
          </div>
          <div className="w-full flex-1 space-y-3">
            <div className="flex gap-2">
              <Input readOnly value={formattedSubUrl} className="font-mono text-xs" />
              <Button
                variant="outline"
                onClick={() => {
                  void navigator.clipboard.writeText(formattedSubUrl)
                  toast.success(`Copied ${subFormat} subscription URL`)
                }}
              >
                <Copy className="size-4" />
                Copy
              </Button>
            </div>
            <div className="flex flex-wrap gap-2">
              {summary.node_usage.map((n) => (
                <Badge key={n.node_id} variant={n.status === 'online' ? 'success' : 'secondary'}>
                  <ShieldCheck className="size-3" />
                  {n.node_name}: {formatBytes(n.uplink_bytes + n.downlink_bytes)}
                </Badge>
              ))}
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">4-Protocol Direct Links ({links.length})</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {links.map((item) => (
            <div
              key={`${item.node_id}-${item.inbound_tag}`}
              className="flex flex-col gap-2 rounded-lg border p-3 sm:flex-row sm:items-center sm:justify-between"
            >
              <div className="min-w-0 space-y-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium">{item.node_name}</span>
                  <Badge variant="outline">{item.protocol}</Badge>
                </div>
                <div className="text-muted-foreground truncate font-mono text-xs">{item.uri}</div>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  void navigator.clipboard.writeText(item.uri)
                  toast.success(`Copied ${item.protocol} URI`)
                }}
              >
                <Copy className="size-3.5" />
                Copy URI
              </Button>
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  )
}
