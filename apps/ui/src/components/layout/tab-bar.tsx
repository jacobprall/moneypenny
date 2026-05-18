import { useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import type { TabRecord } from '@/lib/api-types'
import type { SessionStatus } from '@/lib/api-types'
import { cn } from '@/lib/cn'

function glyphFor(status?: SessionStatus, running?: boolean): string {
  if (running) return '▶'
  if (status === 'paused') return '⏸'
  if (status === 'completed') return '✓'
  if (status === 'failed') return '!'
  if (status === 'archived') return '·'
  return '●'
}

export function TabBar(props: {
  tabs: TabRecord[]
  onActivate: (t: TabRecord) => void
  onClose: (id: string) => void
  onReorder: (next: TabRecord[]) => void
  onNewSession: () => void
}) {
  const sorted = useMemo(() => [...props.tabs].sort((a, b) => a.position - b.position), [props.tabs])
  const [dragId, setDragId] = useState<string | null>(null)

  return (
    <div className="flex items-center gap-1 overflow-x-auto border-b border-border bg-panel px-2 py-1">
      {sorted.map((t) => {
        const running = t.status === 'running' || t.status === 'active'
        const g = glyphFor(t.status, running)
        return (
          <div
            key={t.id}
            draggable
            onDragStart={() => setDragId(t.id)}
            onDragOver={(e) => e.preventDefault()}
            onDrop={() => {
              if (!dragId || dragId === t.id) return
              const order = sorted.map((x) => x.id)
              const from = order.indexOf(dragId)
              const to = order.indexOf(t.id)
              if (from < 0 || to < 0) return
              const nextIds = [...order]
              nextIds.splice(from, 1)
              nextIds.splice(to, 0, dragId)
              const next = nextIds
                .map((id) => sorted.find((x) => x.id === id))
                .filter((x): x is TabRecord => Boolean(x))
                .map((x, i) => ({ ...x, position: i }))
              setDragId(null)
              void props.onReorder(next)
            }}
            className={cn(
              'group flex min-w-[120px] items-center gap-2 border-b-2 px-3 py-2 font-mono text-xs font-medium uppercase tracking-wide',
              t.active ? 'border-accent text-fg' : 'border-transparent text-fg-dim hover:bg-panel-active hover:text-fg',
              running && t.active && 'animate-pulse-border',
            )}
          >
            <button
              type="button"
              className="flex flex-1 items-center gap-2 truncate text-left focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent focus-visible:outline-offset-0"
              onClick={() => void props.onActivate(t)}
            >
              <span aria-hidden className={cn(t.status === 'archived' && 'text-fg-faint')}>
                {g}
              </span>
              <span className="truncate">{t.label}</span>
            </button>
            <button
              type="button"
              className="px-1 text-fg-faint opacity-0 transition-opacity duration-[60ms] group-hover:opacity-100 focus-visible:opacity-100 focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
              aria-label={`close ${t.label}`}
              onClick={() => props.onClose(t.id)}
            >
              ×
            </button>
          </div>
        )
      })}
      <Button type="button" variant="ghost" size="sm" className="shrink-0 uppercase" onClick={props.onNewSession}>
        +
      </Button>
    </div>
  )
}
