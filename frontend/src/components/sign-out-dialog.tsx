import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { api } from '@/lib/api-client'
import { wsManager } from '@/lib/ws-manager'
import { ConfirmDialog } from '@/components/confirm-dialog'

interface SignOutDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function SignOutDialog({ open, onOpenChange }: SignOutDialogProps) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [pending, setPending] = useState(false)

  const handleSignOut = async () => {
    setPending(true)
    wsManager.stop()
    try {
      const res = await api.post<{ end_session_url: string | null }>('/api/v1/auth/logout')
      queryClient.clear()
      if (res.end_session_url) {
        window.location.href = res.end_session_url
        return
      }
    } catch {
      queryClient.clear()
    } finally {
      setPending(false)
      onOpenChange(false)
    }
    void navigate({ to: '/sign-in' })
  }

  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t('auth.signOutConfirmTitle')}
      desc={t('auth.signOutConfirmDesc')}
      confirmText={t('auth.signOutAction')}
      destructive
      isLoading={pending}
      handleConfirm={() => {
        void handleSignOut()
      }}
    />
  )
}
