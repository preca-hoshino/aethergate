import { useEffect, useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { ExternalLink, FileCode2, Info, Palette, RotateCcw, Save, Wrench } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { getActiveLocale, setAppLocale, type AppLocale } from '@/i18n'
import { api } from '@/lib/api-client'
import { cn } from '@/lib/utils'
import {
  useAppStore,
  type SidebarCollapsible,
  type SidebarVariant,
  type ThemeMode,
} from '@/stores/use-app-store'
import type { AboutInfo, AuthProvidersResponse } from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Textarea } from '@/components/ui/textarea'

interface MihomoTemplateDto {
  header_and_dns_yaml: string
  rules_yaml: string
}

type SettingsSection = 'general' | 'mihomo' | 'appearance' | 'about'

export function SettingsPage() {
  const { t, i18n } = useTranslation()
  const [section, setSection] = useState<SettingsSection>('general')

  const theme = useAppStore((s) => s.theme)
  const setTheme = useAppStore((s) => s.setTheme)
  const sidebarVariant = useAppStore((s) => s.sidebarVariant)
  const setSidebarVariant = useAppStore((s) => s.setSidebarVariant)
  const sidebarCollapsible = useAppStore((s) => s.sidebarCollapsible)
  const setSidebarCollapsible = useAppStore((s) => s.setSidebarCollapsible)
  const resetLayout = useAppStore((s) => s.resetLayout)

  const activeLocale = i18n.language.toLowerCase().startsWith('zh') ? 'zh-CN' : getActiveLocale()

  const { data: providers } = useQuery({
    queryKey: ['auth-providers'],
    queryFn: () => api.get<AuthProvidersResponse>('/api/v1/auth/providers'),
  })

  const { data: template } = useQuery({
    queryKey: ['mihomo-template'],
    queryFn: () => api.get<MihomoTemplateDto>('/api/v1/settings/mihomo-template'),
  })

  const { data: aboutInfo } = useQuery({
    queryKey: ['system-about'],
    queryFn: () => api.get<AboutInfo>('/api/v1/system/about'),
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
      toast.success(t('settings.mihomo.savedToast'))
    },
    onError: (err: Error) => {
      toast.error(err.message)
    },
  })

  const navItems: ReadonlyArray<{ id: SettingsSection; title: string; icon: typeof Wrench }> = [
    { id: 'general', title: t('settings.sections.general'), icon: Wrench },
    { id: 'mihomo', title: t('settings.sections.mihomo'), icon: FileCode2 },
    { id: 'appearance', title: t('settings.sections.appearance'), icon: Palette },
    { id: 'about', title: t('settings.sections.about'), icon: Info },
  ]

  return (
    <div className="space-y-2">
      <div className="space-y-0.5">
        <h1 className="text-2xl font-bold tracking-tight md:text-3xl">{t('settings.title')}</h1>
        <p className="text-muted-foreground text-sm">{t('settings.subtitle')}</p>
      </div>

      <Separator className="my-4 lg:my-6" />

      <div className="flex flex-1 flex-col gap-6 lg:flex-row lg:gap-10">
        {/* Responsive Settings Navigation matching shadcn-admin */}
        <aside className="lg:w-56 lg:shrink-0">
          <div className="lg:hidden">
            <Select
              value={section}
              onValueChange={(v) => {
                setSection(v as SettingsSection)
              }}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {navItems.map((item) => (
                  <SelectItem key={item.id} value={item.id}>
                    {item.title}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <nav className="hidden flex-col gap-1 lg:flex">
            {navItems.map((item) => {
              const Icon = item.icon
              const active = section === item.id
              return (
                <Button
                  key={item.id}
                  type="button"
                  variant="ghost"
                  className={cn(
                    'justify-start',
                    active ? 'bg-muted hover:bg-muted font-medium' : 'hover:bg-accent',
                  )}
                  onClick={() => {
                    setSection(item.id)
                  }}
                >
                  <Icon className="size-4" />
                  <span>{item.title}</span>
                </Button>
              )
            })}
          </nav>
        </aside>

        <div className="min-w-0 flex-1 space-y-6">
          {section === 'general' && (
            <div className="grid gap-4 md:grid-cols-2">
              <Card>
                <CardHeader>
                  <CardTitle>{t('settings.oidc.title')}</CardTitle>
                  <CardDescription>{t('settings.oidc.desc')}</CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 text-sm">
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">{t('settings.oidc.status')}</span>
                    <Badge variant={providers?.oidc_enabled ? 'success' : 'secondary'}>
                      {providers?.oidc_enabled ? t('common.enabled') : t('common.disabled')}
                    </Badge>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">{t('settings.oidc.provider')}</span>
                    <span className="font-medium">{providers?.provider_name ?? '—'}</span>
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle>{t('settings.telemetry.title')}</CardTitle>
                  <CardDescription>{t('settings.telemetry.desc')}</CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 text-sm">
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">{t('settings.telemetry.idleCadence')}</span>
                    <span className="font-mono text-xs">{t('settings.telemetry.idleCadenceValue')}</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">{t('settings.telemetry.realtimeCadence')}</span>
                    <span className="font-mono text-xs">{t('settings.telemetry.realtimeCadenceValue')}</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">{t('settings.telemetry.resetPolicy')}</span>
                    <span className="font-mono text-xs">{t('settings.telemetry.resetPolicyValue')}</span>
                  </div>
                </CardContent>
              </Card>
            </div>
          )}

          {section === 'mihomo' && (
            <Card>
              <CardHeader>
                <CardTitle>{t('settings.mihomo.title')}</CardTitle>
                <CardDescription>{t('settings.mihomo.desc')}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-1.5">
                  <Label>{t('settings.mihomo.headerDns')}</Label>
                  <Textarea
                    rows={12}
                    value={headerDns}
                    onChange={(e) => {
                      setHeaderDns(e.target.value)
                    }}
                    className="font-mono text-xs"
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{t('settings.mihomo.rulesYaml')}</Label>
                  <Textarea
                    rows={10}
                    value={rulesYaml}
                    onChange={(e) => {
                      setRulesYaml(e.target.value)
                    }}
                    className="font-mono text-xs"
                  />
                </div>
                <Button
                  disabled={saveMutation.isPending}
                  onClick={() => {
                    saveMutation.mutate()
                  }}
                >
                  <Save className="size-4" />
                  <span>{t('settings.mihomo.saveBtn')}</span>
                </Button>
              </CardContent>
            </Card>
          )}

          {section === 'appearance' && (
            <Card className="max-w-2xl">
              <CardHeader>
                <CardTitle>{t('settings.appearance.title')}</CardTitle>
                <CardDescription>{t('settings.appearance.desc')}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="space-y-2">
                  <Label>{t('settings.appearance.languageLabel')}</Label>
                  <p className="text-muted-foreground text-xs">{t('settings.appearance.languageDesc')}</p>
                  <Select
                    value={activeLocale}
                    onValueChange={(v) => {
                      void setAppLocale(v as AppLocale)
                    }}
                  >
                    <SelectTrigger className="sm:w-72">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="zh-CN">{t('language.zhCN')}</SelectItem>
                      <SelectItem value="en">{t('language.en')}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-2">
                  <Label>{t('settings.appearance.themeLabel')}</Label>
                  <p className="text-muted-foreground text-xs">{t('settings.appearance.themeDesc')}</p>
                  <Select
                    value={theme}
                    onValueChange={(v) => {
                      setTheme(v as ThemeMode)
                    }}
                  >
                    <SelectTrigger className="sm:w-72">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="light">{t('theme.light')}</SelectItem>
                      <SelectItem value="dark">{t('theme.dark')}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="grid gap-4 sm:grid-cols-2">
                  <div className="space-y-2">
                    <Label>{t('settings.appearance.sidebarVariantLabel')}</Label>
                    <p className="text-muted-foreground text-xs">
                      {t('settings.appearance.sidebarVariantDesc')}
                    </p>
                    <Select
                      value={sidebarVariant}
                      onValueChange={(v) => {
                        setSidebarVariant(v as SidebarVariant)
                      }}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="inset">{t('settings.appearance.variants.inset')}</SelectItem>
                        <SelectItem value="sidebar">{t('settings.appearance.variants.sidebar')}</SelectItem>
                        <SelectItem value="floating">{t('settings.appearance.variants.floating')}</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>

                  <div className="space-y-2">
                    <Label>{t('settings.appearance.collapsibleLabel')}</Label>
                    <Select
                      value={sidebarCollapsible}
                      onValueChange={(v) => {
                        setSidebarCollapsible(v as SidebarCollapsible)
                      }}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="icon">{t('settings.appearance.collapsible.icon')}</SelectItem>
                        <SelectItem value="offcanvas">{t('settings.appearance.collapsible.offcanvas')}</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>

                <div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      resetLayout()
                      toast.success(t('settings.appearance.layoutResetToast'))
                    }}
                  >
                    <RotateCcw className="size-3.5" />
                    <span>{t('settings.appearance.resetLayout')}</span>
                  </Button>
                </div>
              </CardContent>
            </Card>
          )}

          {section === 'about' && (
            <Card className="max-w-2xl">
              <CardHeader>
                <CardTitle>{t('settings.about.title')}</CardTitle>
                <CardDescription>{t('settings.about.desc')}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3 text-sm">
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">{t('settings.about.version')}</span>
                  <span className="font-mono font-medium">
                    v{aboutInfo?.version ?? '0.1.0'}
                  </span>
                </div>
                <Separator />
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">{t('settings.about.commit')}</span>
                  <span className="font-mono text-xs" title={aboutInfo?.commit_full}>
                    {aboutInfo?.commit ?? '—'}
                  </span>
                </div>
                <Separator />
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">{t('settings.about.license')}</span>
                  <Badge variant="outline">{aboutInfo?.license ?? 'AGPL-3.0'}</Badge>
                </div>
                <Separator />
                <div className="flex items-center justify-between gap-4">
                  <span className="text-muted-foreground shrink-0">
                    {t('settings.about.repository')}
                  </span>
                  <a
                    href={aboutInfo?.repository_url ?? 'https://github.com/preca-hoshino/aethergate'}
                    target="_blank"
                    rel="noreferrer"
                    className="text-primary inline-flex min-w-0 items-center gap-1.5 font-mono text-xs hover:underline"
                  >
                    <span className="truncate">
                      {aboutInfo?.repository_url ?? 'https://github.com/preca-hoshino/aethergate'}
                    </span>
                    <ExternalLink className="size-3.5 shrink-0" />
                  </a>
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  )
}
