import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Clock, Copy, QrCode, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'
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
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'

export function PortalPage() {
  const { t } = useTranslation()
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
    return (
      <div className="space-y-6">
        <Skeleton className="h-12 w-64" />
        <div className="grid gap-4 sm:grid-cols-3">
          <Skeleton className="h-24 rounded-xl" />
          <Skeleton className="h-24 rounded-xl" />
          <Skeleton className="h-24 rounded-xl" />
        </div>
        <Skeleton className="h-52 w-full rounded-xl" />
      </div>
    )
  }

  if (!summary.proxy_enabled) {
    return (
      <div className="mx-auto max-w-xl py-10">
        <Card>
          <CardHeader>
            <div className="mb-2 flex size-10 items-center justify-center rounded-lg bg-amber-500/15 text-amber-600 dark:text-amber-400">
              <Clock className="size-5" />
            </div>
            <CardTitle>{t('portal.pendingTitle')}</CardTitle>
            <CardDescription>{t('portal.pendingDesc', { username: summary.proxy_username })}</CardDescription>
          </CardHeader>
        </Card>
      </div>
    )
  }

  const formattedSubUrl =
    subFormat === 'uri' ? summary.subscription_url : `${summary.subscription_url}?format=${subFormat}`

  const formatLabel =
    subFormat === 'mihomo'
      ? t('portal.subscription.formats.mihomo')
      : subFormat === 'singbox'
        ? t('portal.subscription.formats.singbox')
        : t('portal.subscription.formats.uri')

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{t('portal.title')}</h1>
        <p className="text-muted-foreground text-sm">
          {t('portal.subtitle', { month, username: summary.proxy_username })}
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">{t('portal.kpi.monthlyTotal')}</CardTitle>
          </CardHeader>
          <CardContent className="text-2xl font-bold">
            {formatBytes(summary.uplink_bytes + summary.downlink_bytes)}
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">{t('portal.kpi.uplinkDownlink')}</CardTitle>
          </CardHeader>
          <CardContent className="font-mono text-sm">
            ↑ {formatBytes(summary.uplink_bytes)} · ↓ {formatBytes(summary.downlink_bytes)}
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">{t('portal.kpi.peakBandwidth')}</CardTitle>
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
              <span>{t('portal.subscription.title')}</span>
            </CardTitle>
            <CardDescription>{t('portal.subscription.desc')}</CardDescription>
          </div>
          <Tabs
            value={subFormat}
            onValueChange={(v) => {
              setSubFormat(v as typeof subFormat)
            }}
          >
            <TabsList>
              <TabsTrigger value="mihomo">{t('portal.subscription.formats.mihomo')}</TabsTrigger>
              <TabsTrigger value="singbox">{t('portal.subscription.formats.singbox')}</TabsTrigger>
              <TabsTrigger value="uri">{t('portal.subscription.formats.uri')}</TabsTrigger>
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
                  toast.success(t('portal.subscription.copiedToast', { format: formatLabel }))
                }}
              >
                <Copy className="size-4" />
                <span>{t('common.copy')}</span>
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
          <CardTitle className="text-base">{t('portal.directLinks.title', { count: links.length })}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {links.length === 0 ? (
            <p className="text-muted-foreground py-6 text-center text-sm">{t('portal.directLinks.empty')}</p>
          ) : (
            links.map((item) => (
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
                    toast.success(t('portal.directLinks.copiedToast', { protocol: item.protocol }))
                  }}
                >
                  <Copy className="size-3.5" />
                  <span>{t('portal.directLinks.copyUri')}</span>
                </Button>
              </div>
            ))
          )}
        </CardContent>
      </Card>
    </div>
  )
}
