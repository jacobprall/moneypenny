import type { ServerEvent } from '@/lib/api-types'

export function ActivityFeed(props: { events: ServerEvent[] }) {
  const lines = props.events.slice(-50)
  return (
    <div className="border border-border bg-canvas">
      <div className="border-b border-border px-4 py-2 font-mono text-xs font-semibold uppercase tracking-wide text-fg">
        activity (last 50)
      </div>
      <div className="max-h-64 overflow-auto p-2 font-mono text-[11px] text-fg-dim">
        {lines.length === 0 ? (
          <div className="p-4 text-fg-faint">no events yet</div>
        ) : (
          lines.map((e) => (
            <div key={e.id} className="border-b border-border py-1 last:border-0">
              {e.type} · {e.session_id ?? '—'} · {JSON.stringify(e.detail)}
            </div>
          ))
        )}
      </div>
    </div>
  )
}
