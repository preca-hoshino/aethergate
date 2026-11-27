import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import { en } from './locales/en'
import { zhCN } from './locales/zh-CN'

export const SUPPORTED_LOCALES = ['zh-CN', 'en'] as const
export type AppLocale = (typeof SUPPORTED_LOCALES)[number]
export const LOCALE_STORAGE_KEY = 'ag-locale'

function normalizeLocale(raw: string | null | undefined): AppLocale | null {
  if (!raw) return null
  const lower = raw.toLowerCase()
  if (lower.startsWith('zh')) return 'zh-CN'
  if (lower.startsWith('en')) return 'en'
  return null
}

export function resolveInitialLocale(): AppLocale {
  const saved = normalizeLocale(localStorage.getItem(LOCALE_STORAGE_KEY))
  if (saved) return saved

  const browserCandidates = [...navigator.languages, navigator.language]
  for (const candidate of browserCandidates) {
    const matched = normalizeLocale(candidate)
    if (matched) return matched
  }
  return 'zh-CN'
}

export function syncDocumentLocale(locale: AppLocale): void {
  document.documentElement.lang = locale
  document.title = i18n.t('brand.pageTitle')
}

export async function setAppLocale(locale: AppLocale): Promise<void> {
  localStorage.setItem(LOCALE_STORAGE_KEY, locale)
  await i18n.changeLanguage(locale)
  syncDocumentLocale(locale)
}

export function getActiveLocale(): AppLocale {
  return normalizeLocale(i18n.resolvedLanguage ?? i18n.language) ?? 'zh-CN'
}

const initialLocale = resolveInitialLocale()

void i18n.use(initReactI18next).init({
  resources: {
    'zh-CN': { translation: zhCN },
    en: { translation: en },
  },
  lng: initialLocale,
  fallbackLng: 'en',
  interpolation: {
    escapeValue: false,
  },
})

syncDocumentLocale(initialLocale)

i18n.on('languageChanged', (lng) => {
  syncDocumentLocale(normalizeLocale(lng) ?? 'zh-CN')
})

export { i18n }
