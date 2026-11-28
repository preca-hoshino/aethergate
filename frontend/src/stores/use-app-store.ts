import { create } from 'zustand'

export type ThemeMode = 'light' | 'dark'
export type SidebarVariant = 'inset' | 'sidebar' | 'floating'
export type SidebarCollapsible = 'icon' | 'offcanvas'

const THEME_STORAGE_KEY = 'ag-theme'
const VARIANT_STORAGE_KEY = 'ag-sidebar-variant'
const COLLAPSIBLE_STORAGE_KEY = 'ag-sidebar-collapsible'

const DEFAULT_VARIANT: SidebarVariant = 'inset'
const DEFAULT_COLLAPSIBLE: SidebarCollapsible = 'icon'

function currentYearMonth(): string {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  return `${y}-${m}`
}

function getSystemPreferredTheme(): ThemeMode {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function parseTheme(raw: string | null): ThemeMode {
  if (raw === 'light' || raw === 'dark') return raw
  return getSystemPreferredTheme()
}

function parseVariant(raw: string | null): SidebarVariant {
  if (raw === 'inset' || raw === 'sidebar' || raw === 'floating') return raw
  return DEFAULT_VARIANT
}

function parseCollapsible(raw: string | null): SidebarCollapsible {
  if (raw === 'icon' || raw === 'offcanvas') return raw
  return DEFAULT_COLLAPSIBLE
}

export function applyThemeToDom(mode: ThemeMode): void {
  const root = document.documentElement
  const isDark = mode === 'dark'
  root.classList.toggle('dark', isDark)
  root.classList.toggle('light', !isDark)

  const themeColor = isDark ? '#020817' : '#ffffff'
  const metaThemeColor = document.querySelector("meta[name='theme-color']")
  if (metaThemeColor) {
    metaThemeColor.setAttribute('content', themeColor)
  }
}

interface AppUiState {
  theme: ThemeMode
  sidebarVariant: SidebarVariant
  sidebarCollapsible: SidebarCollapsible
  selectedMonth: string
  commandOpen: boolean
  setTheme: (theme: ThemeMode) => void
  setSidebarVariant: (variant: SidebarVariant) => void
  setSidebarCollapsible: (collapsible: SidebarCollapsible) => void
  resetLayout: () => void
  setSelectedMonth: (month: string) => void
  setCommandOpen: (open: boolean) => void
}

export const useAppStore = create<AppUiState>((set) => ({
  theme: parseTheme(localStorage.getItem(THEME_STORAGE_KEY)),
  sidebarVariant: parseVariant(localStorage.getItem(VARIANT_STORAGE_KEY)),
  sidebarCollapsible: parseCollapsible(localStorage.getItem(COLLAPSIBLE_STORAGE_KEY)),
  selectedMonth: currentYearMonth(),
  commandOpen: false,
  setTheme: (theme) => {
    localStorage.setItem(THEME_STORAGE_KEY, theme)
    applyThemeToDom(theme)
    set({ theme })
  },
  setSidebarVariant: (sidebarVariant) => {
    localStorage.setItem(VARIANT_STORAGE_KEY, sidebarVariant)
    set({ sidebarVariant })
  },
  setSidebarCollapsible: (sidebarCollapsible) => {
    localStorage.setItem(COLLAPSIBLE_STORAGE_KEY, sidebarCollapsible)
    set({ sidebarCollapsible })
  },
  resetLayout: () => {
    localStorage.setItem(VARIANT_STORAGE_KEY, DEFAULT_VARIANT)
    localStorage.setItem(COLLAPSIBLE_STORAGE_KEY, DEFAULT_COLLAPSIBLE)
    set({
      sidebarVariant: DEFAULT_VARIANT,
      sidebarCollapsible: DEFAULT_COLLAPSIBLE,
    })
  },
  setSelectedMonth: (selectedMonth) => {
    set({ selectedMonth })
  },
  setCommandOpen: (commandOpen) => {
    set({ commandOpen })
  },
}))
