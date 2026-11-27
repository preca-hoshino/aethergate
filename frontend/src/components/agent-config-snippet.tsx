import { Copy, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'

interface AgentConfigSnippetProps {
  nodeId: string
  hmacSecret: string
  notice?: string
  onRotateSecret?: () => void
  isRotating?: boolean
}

export function AgentConfigSnippet({
  nodeId,
  hmacSecret,
  notice,
  onRotateSecret,
  isRotating = false,
}: AgentConfigSnippetProps) {
  const { t } = useTranslation()
  const snippet = `node_id = "${nodeId}"\nhmac_secret = "${hmacSecret}"\nserver_url = "${window.location.origin}"`

  return (
    <div className="min-w-0 space-y-3 text-sm">
      {notice && <p className="font-medium text-emerald-600 dark:text-emerald-400">{notice}</p>}
      <pre className="bg-muted max-w-full overflow-x-auto rounded-md p-3 font-mono text-xs">{snippet}</pre>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => {
            void navigator.clipboard.writeText(snippet)
            toast.success(t('nodes.copiedSnippet'))
          }}
        >
          <Copy className="size-3.5" />
          <span>{t('nodes.copySnippet')}</span>
        </Button>
        {onRotateSecret && (
          <Button type="button" variant="outline" size="sm" disabled={isRotating} onClick={onRotateSecret}>
            <RefreshCw className="size-3.5" />
            <span>{t('nodes.rotateSecret')}</span>
          </Button>
        )}
      </div>
    </div>
  )
}
