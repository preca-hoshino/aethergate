import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Calendar, ChevronLeft, ChevronRight, LineChart as LineChartIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { api } from '@/lib/api-client'
import { currentStatMonth, formatBytes } from '@/lib/formatters'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/stores/use-app-store'
import type { UsersMonthlyTimeSeries, UserTimeSeriesUser } from '@/types/api'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'

type MetricKind = 'total' | 'uplink' | 'downlink'
type AggregationMode = 'hourly' | 'cumulative'

const HOUR_MS = 3_600_000

const USER_LINE_COLORS = [
  '#2563eb',
  '#10b981',
  '#f59e0b',
  '#ef4444',
  '#8b5cf6',
  '#ec4899',
  '#06b6d4',
  '#84cc16',
  '#f97316',
  '#6366f1',
  '#14b8a6',
  '#d946ef',
] as const

function shiftYearMonth(ym: string, deltaMonths: number): string {
  const parts = ym.split('-')
  const year = Number(parts[0])
  const month = Number(parts[1])
  if (!Number.isFinite(year) || !Number.isFinite(month)) {
    return currentStatMonth()
  }
  const d = new Date(year, month - 1 + deltaMonths, 1)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`
}

function buildMonthOptions(selectedMonth: string, count = 12): string[] {
  const current = currentStatMonth()
  const months: string[] = []
  for (let i = 0; i < count; i += 1) {
    months.push(shiftYearMonth(current, -i))
  }
  if (!months.includes(selectedMonth)) {
    months.push(selectedMonth)
    months.sort((a, b) => b.localeCompare(a))
  }
  return months
}

function formatHourAxisLabel(epochMs: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(new Date(epochMs))
}

function formatHourTooltipLabel(epochMs: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(new Date(epochMs))
}

interface TooltipEntry {
  dataKey?: string | number
  value?: number | string | ReadonlyArray<number | string>
  color?: string
}

interface CustomTooltipProps {
  active?: boolean
  payload?: TooltipEntry[]
  label?: string
  userMap: Map<string, UserTimeSeriesUser>
}

function HourlyTrafficTooltip({ active, payload, label, userMap }: CustomTooltipProps) {
  if (!active || !payload || payload.length === 0) {
    return null
  }

  const sorted = [...payload]
    .filter((item): item is TooltipEntry & { value: number } => typeof item.value === 'number' && item.value > 0)
    .sort((a, b) => b.value - a.value)

  const displayItems = sorted.length > 0 ? sorted : payload.slice(0, 8)

  return (
    <div className="bg-popover text-popover-foreground border-border min-w-52 rounded-lg border p-3 text-xs shadow-md">
      <div className="mb-2 font-mono font-semibold">{label}</div>
      <div className="space-y-1">
        {displayItems.map((item) => {
          const uid = String(item.dataKey ?? '')
          const user = userMap.get(uid)
          const name = user ? user.display_name : uid
          const val = typeof item.value === 'number' ? item.value : 0
          return (
            <div key={uid} className="flex items-center justify-between gap-4">
              <div className="flex min-w-0 items-center gap-1.5">
                <span
                  className="inline-block size-2 shrink-0 rounded-full"
                  style={{ backgroundColor: item.color ?? '#2563eb' }}
                />
                <span className="truncate">{name}</span>
              </div>
              <span className="font-mono font-medium">{formatBytes(val)}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}

export function DataBoardPage() {
  const { t, i18n } = useTranslation()
  const month = useAppStore((s) => s.selectedMonth)
  const setSelectedMonth = useAppStore((s) => s.setSelectedMonth)

  const [metric, setMetric] = useState<MetricKind>('total')
  const [mode, setMode] = useState<AggregationMode>('hourly')
  const [hiddenUserIds, setHiddenUserIds] = useState<Set<string>>(() => new Set())

  const currentMonth = currentStatMonth()
  const monthOptions = useMemo(() => buildMonthOptions(month), [month])

  const { data, isLoading } = useQuery({
    queryKey: ['analytics-users-timeseries', month],
    queryFn: () =>
      api.get<UsersMonthlyTimeSeries>(`/api/v1/analytics/users-timeseries?month=${month}`),
  })

  const userMap = useMemo(() => {
    const map = new Map<string, UserTimeSeriesUser>()
    for (const u of data?.users ?? []) {
      map.set(u.user_id, u)
    }
    return map
  }, [data])

  const userColorMap = useMemo(() => {
    const map = new Map<string, string>()
    ;(data?.users ?? []).forEach((u, idx) => {
      map.set(u.user_id, USER_LINE_COLORS[idx % USER_LINE_COLORS.length] ?? '#2563eb')
    })
    return map
  }, [data])

  const visibleUsers = useMemo(
    () => (data?.users ?? []).filter((u) => !hiddenUserIds.has(u.user_id)),
    [data, hiddenUserIds],
  )

  const chartData = useMemo(() => {
    if (!data || data.users.length === 0) return []

    const startMs = Math.floor(Date.parse(data.start_at) / HOUR_MS) * HOUR_MS
    const endMs = Math.floor(Date.parse(data.end_at) / HOUR_MS) * HOUR_MS
    if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs <= startMs) return []

    const nowNextHourMs = Math.ceil(Date.now() / HOUR_MS) * HOUR_MS
    const effectiveEndMs = Math.max(startMs + HOUR_MS, Math.min(endMs, nowNextHourMs))

    const bucketMap = new Map<number, Map<string, number>>()
    for (const p of data.points) {
      const ts = Math.floor(Date.parse(p.bucket) / HOUR_MS) * HOUR_MS
      let perUser = bucketMap.get(ts)
      if (!perUser) {
        perUser = new Map<string, number>()
        bucketMap.set(ts, perUser)
      }
      const val =
        metric === 'uplink'
          ? p.uplink_bytes
          : metric === 'downlink'
            ? p.downlink_bytes
            : p.uplink_bytes + p.downlink_bytes
      perUser.set(p.user_id, (perUser.get(p.user_id) ?? 0) + val)
    }

    const runningTotals = new Map<string, number>()
    const rows: Array<Record<string, number | string>> = []

    for (let ts = startMs; ts < effectiveEndMs; ts += HOUR_MS) {
      const perUser = bucketMap.get(ts)
      const row: Record<string, number | string> = {
        label: formatHourAxisLabel(ts, i18n.language),
        tooltipLabel: formatHourTooltipLabel(ts, i18n.language),
      }
      for (const u of data.users) {
        const inc = perUser?.get(u.user_id) ?? 0
        if (mode === 'cumulative') {
          const nextTotal = (runningTotals.get(u.user_id) ?? 0) + inc
          runningTotals.set(u.user_id, nextTotal)
          row[u.user_id] = nextTotal
        } else {
          row[u.user_id] = inc
        }
      }
      rows.push(row)
    }

    return rows
  }, [data, metric, mode, i18n.language])

  const totalMonthBytes = useMemo(
    () =>
      (data?.users ?? []).reduce((sum, u) => {
        const val =
          metric === 'uplink'
            ? u.total_uplink_bytes
            : metric === 'downlink'
              ? u.total_downlink_bytes
              : u.total_uplink_bytes + u.total_downlink_bytes
        return sum + val
      }, 0),
    [data, metric],
  )

  const toggleUser = (userId: string) => {
    setHiddenUserIds((prev) => {
      const next = new Set(prev)
      if (next.has(userId)) {
        next.delete(userId)
      } else {
        next.add(userId)
      }
      return next
    })
  }

  const hasAnyTraffic = (data?.points.length ?? 0) > 0

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="flex items-center gap-2 text-2xl font-bold tracking-tight">
            <LineChartIcon className="size-6" />
            <span>{t('dataBoard.title')}</span>
          </h1>
          <p className="text-muted-foreground text-sm">{t('dataBoard.subtitle', { month })}</p>
        </div>

        <div className="flex items-center gap-1.5">
          <Button
            variant="outline"
            size="icon"
            title={t('dataBoard.prevMonth')}
            aria-label={t('dataBoard.prevMonth')}
            onClick={() => {
              setSelectedMonth(shiftYearMonth(month, -1))
            }}
          >
            <ChevronLeft className="size-4" />
          </Button>
          <Select
            value={month}
            onValueChange={(value) => {
              setSelectedMonth(value)
            }}
          >
            <SelectTrigger className="w-36 font-mono text-xs" aria-label={t('dataBoard.monthLabel')}>
              <Calendar className="text-muted-foreground me-1.5 size-3.5" />
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {monthOptions.map((m) => (
                <SelectItem key={m} value={m} className="font-mono text-xs">
                  {m}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant="outline"
            size="icon"
            title={t('dataBoard.nextMonth')}
            aria-label={t('dataBoard.nextMonth')}
            disabled={month >= currentMonth}
            onClick={() => {
              setSelectedMonth(shiftYearMonth(month, 1))
            }}
          >
            <ChevronRight className="size-4" />
          </Button>
        </div>
      </div>

      <Card>
        <CardHeader className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
          <div className="space-y-1">
            <CardTitle>{t('dataBoard.chartTitle')}</CardTitle>
            <CardDescription>{t('dataBoard.chartDesc', { month })}</CardDescription>
            <div className="text-muted-foreground flex flex-wrap items-center gap-3 pt-1 text-xs">
              <span className="font-medium">
                {t('dataBoard.summaryTotal', { total: formatBytes(totalMonthBytes) })}
              </span>
              <span>·</span>
              <span>{t('dataBoard.summaryUsers', { count: data?.users.length ?? 0 })}</span>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Tabs
              value={mode}
              onValueChange={(v) => {
                setMode(v as AggregationMode)
              }}
            >
              <TabsList>
                <TabsTrigger value="hourly">{t('dataBoard.modes.hourly')}</TabsTrigger>
                <TabsTrigger value="cumulative">{t('dataBoard.modes.cumulative')}</TabsTrigger>
              </TabsList>
            </Tabs>

            <Tabs
              value={metric}
              onValueChange={(v) => {
                setMetric(v as MetricKind)
              }}
            >
              <TabsList>
                <TabsTrigger value="total">{t('dataBoard.metrics.total')}</TabsTrigger>
                <TabsTrigger value="uplink">{t('dataBoard.metrics.uplink')}</TabsTrigger>
                <TabsTrigger value="downlink">{t('dataBoard.metrics.downlink')}</TabsTrigger>
              </TabsList>
            </Tabs>
          </div>
        </CardHeader>

        <CardContent className="space-y-5">
          {isLoading ? (
            <Skeleton className="h-[480px] w-full rounded-lg" />
          ) : !hasAnyTraffic || chartData.length === 0 ? (
            <div className="text-muted-foreground flex h-[480px] items-center justify-center rounded-lg border border-dashed text-sm">
              {t('dataBoard.empty', { month })}
            </div>
          ) : (
            <div className="h-[460px] w-full sm:h-[540px]">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={chartData} margin={{ top: 10, right: 16, left: 8, bottom: 8 }}>
                  <CartesianGrid strokeDasharray="3 3" className="stroke-border" vertical={false} />
                  <XAxis
                    dataKey="label"
                    stroke="#888888"
                    fontSize={12}
                    tickLine={false}
                    axisLine={false}
                    minTickGap={48}
                  />
                  <YAxis
                    stroke="#888888"
                    fontSize={12}
                    tickLine={false}
                    axisLine={false}
                    width={76}
                    tickFormatter={(v: number) => formatBytes(v)}
                  />
                  <Tooltip
                    content={({ active, payload }) => {
                      const first = payload?.[0]
                      const rowPayload = first?.payload as Record<string, number | string> | undefined
                      const tipLabel = String(rowPayload?.tooltipLabel ?? '')
                      return (
                        <HourlyTrafficTooltip
                          active={active}
                          payload={payload}
                          label={tipLabel}
                          userMap={userMap}
                        />
                      )
                    }}
                  />
                  {visibleUsers.map((u) => (
                    <Line
                      key={u.user_id}
                      type="monotone"
                      dataKey={u.user_id}
                      name={u.display_name}
                      stroke={userColorMap.get(u.user_id) ?? '#2563eb'}
                      strokeWidth={2}
                      dot={false}
                      activeDot={{ r: 4 }}
                      isAnimationActive={false}
                    />
                  ))}
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}

          {(data?.users.length ?? 0) > 0 && (
            <div className="flex flex-wrap items-center gap-2 border-t pt-4">
              {(data?.users ?? []).map((u) => {
                const hidden = hiddenUserIds.has(u.user_id)
                const color = userColorMap.get(u.user_id) ?? '#2563eb'
                const userTotal =
                  metric === 'uplink'
                    ? u.total_uplink_bytes
                    : metric === 'downlink'
                      ? u.total_downlink_bytes
                      : u.total_uplink_bytes + u.total_downlink_bytes
                return (
                  <button
                    key={u.user_id}
                    type="button"
                    onClick={() => {
                      toggleUser(u.user_id)
                    }}
                    className={cn(
                      'inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs transition-opacity',
                      hidden ? 'opacity-40' : 'bg-muted/50 hover:bg-muted',
                    )}
                  >
                    <span
                      className="inline-block size-2.5 rounded-full"
                      style={{ backgroundColor: color }}
                    />
                    <span className="font-medium">{u.display_name}</span>
                    <span className="text-muted-foreground font-mono">
                      {formatBytes(userTotal)}
                    </span>
                  </button>
                )
              })}

              {hiddenUserIds.size > 0 && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2 text-xs"
                  onClick={() => {
                    setHiddenUserIds(new Set())
                  }}
                >
                  {t('dataBoard.showAllUsers')}
                </Button>
              )}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
