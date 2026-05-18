import { useQuery } from '@tanstack/react-query'
import { Card } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { formatUsd } from '@/lib/format'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'

export function StatTiles() {
  const q = useQuery({ queryKey: queryKeys.health(), queryFn: () => api.system.health() })
  if (q.isLoading) {
    return (
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} className="h-24" />
        ))}
      </div>
    )
  }
  const h = q.data
  return (
    <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
      <Card variant="default" className="space-y-1">
        <div className="text-xs font-semibold uppercase tracking-wide text-fg-dim">today cost</div>
        <div className="font-mono text-4xl font-medium tabular-nums text-fg">
          {formatUsd(h?.today_cost_usd)}
        </div>
      </Card>
      <Card variant="default" className="space-y-1">
        <div className="text-xs font-semibold uppercase tracking-wide text-fg-dim">active sessions</div>
        <div className="font-mono text-4xl font-medium tabular-nums text-fg">{h?.active_sessions ?? 0}</div>
      </Card>
      <Card variant="default" className="space-y-1">
        <div className="text-xs font-semibold uppercase tracking-wide text-fg-dim">pending work</div>
        <div className="font-mono text-4xl font-medium tabular-nums text-fg">{h?.pending_work ?? 0}</div>
      </Card>
      <Card variant="default" className="space-y-1">
        <div className="text-xs font-semibold uppercase tracking-wide text-fg-dim">total knowledge</div>
        <div className="font-mono text-4xl font-medium tabular-nums text-fg">{h?.total_knowledge ?? 0}</div>
      </Card>
    </div>
  )
}
