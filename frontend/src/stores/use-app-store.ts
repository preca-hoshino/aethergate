import { create } from 'zustand'

export type ThemeMode = 'light' | 'dark' | 'system'

function currentYearMonth(): string {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  return `${y}-${m}`
}

export function applyThemeToDom(mode: ThemeMode): void {
  const root = document.documentElement
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
  const isDark = mode === 'dark' || (mode === 'system' && prefersDark)
  root.classList.toggle('dark', isDark)
}

interface AppUiState {
  theme: ThemeMode
  selectedMonth: string
  sidebarCollapsed: boolean
  commandOpen: boolean
  setTheme: (theme: ThemeMode) => void
  setSelectedMonth: (month: string) => void
  toggleSidebar: () => void
  setCommandOpen: (open: boolean) => void
}

export const useAppStore = create<AppUiState>((set) => ({
  theme: (localStorage.getItem('ag-theme') as ThemeMode | null) ?? 'system',
  selectedMonth: currentYearMonth(),
  sidebarCollapsed: false,
  commandOpen: false,
  setTheme: (theme) => {
    localStorage.setItem('ag-theme', theme)
    applyThemeToDom(theme)
    set({ theme })
  },
  setSelectedMonth: (selectedMonth) => {
    set({ selectedMonth })
  },
  toggleSidebar: () => {
    set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed }))
  },
  setCommandOpen: (commandOpen) => {
    set({ commandOpen })
  },
}))
