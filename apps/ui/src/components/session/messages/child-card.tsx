import { useNavigate } from '@tanstack/react-router'
import { useTabs } from '@/hooks/use-tabs'
import { formatDurationMs } from '@/lib/format'

export function ChildCard(props: { meta: Record<string, unknown> }) {
  const { openTab } = useTabs()
  const navigate = useNavigate()
  const childId = typeof props.meta.child_id === 'string' ? props.meta.child_id : ''
  const label = typeof props.meta.label === 'string' ? props.meta.label : childId
  const status = typeof props.meta.status === 'string' ? props.meta.status : 'active'
  const elapsed = typeof props.meta.elapsed_ms === 'number' ? props.meta.elapsed_ms : undefined

  return (
    <button
      type="button"
      className="w-full border border-border-bold bg-panel-active px-3 py-2 text-left font-mono text-xs text-fg focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
      onClick={async () => {
        if (!childId) return
        await openTab({ kind: 'session', session_id: childId, label: label ?? 'child' })
        navigate({ to: '/s/$sessionId', params: { sessionId: childId } })
      }}
    >
      child · {label} · {status} · {formatDurationMs(elapsed)}
    </button>
  )
}
