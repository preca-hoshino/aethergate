import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { AuditLogRow } from '@/types/api'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'

export function AuditLogsPage() {
  const { data: logs = [] } = useQuery({
    queryKey: ['audit-logs'],
    queryFn: () => api.get<AuditLogRow[]>('/api/v1/audit-logs'),
  })

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Audit Logs</h1>
        <p className="text-muted-foreground text-sm">Recent administrative actions across nodes, users, and grants.</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Latest Events ({logs.length})</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Time</TableHead>
                <TableHead>Actor</TableHead>
                <TableHead>Action</TableHead>
                <TableHead>Target</TableHead>
                <TableHead>Detail</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {logs.map((row) => (
                <TableRow key={row.id}>
                  <TableCell className="font-mono text-xs">
                    {new Date(row.created_at).toLocaleString()}
                  </TableCell>
                  <TableCell>{row.actor_name ?? 'system'}</TableCell>
                  <TableCell>
                    <Badge variant="outline" className="font-mono">
                      {row.action}
                    </Badge>
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {row.target_type} {row.target_id ? `(${row.target_id.slice(0, 8)})` : ''}
                  </TableCell>
                  <TableCell className="text-muted-foreground max-w-xs truncate font-mono text-xs">
                    {JSON.stringify(row.detail)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  )
}
