import { useEffect } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { Toaster } from '@/components/ui/sonner'
import { router } from '@/router'
import { applyThemeToDom, useAppStore } from '@/stores/use-app-store'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 10_000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
})

export function App() {
  const theme = useAppStore((s) => s.theme)

  useEffect(() => {
    applyThemeToDom(theme)
  }, [theme])

  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      <Toaster richColors position="top-right" />
    </QueryClientProvider>
  )
}
