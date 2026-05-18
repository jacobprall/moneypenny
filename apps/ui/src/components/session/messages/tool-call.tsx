import { useEffect, useMemo, useState } from 'react'
import type { Message } from '@/lib/api-types'
import { formatDurationMs } from '@/lib/format'
import { truncateUtf8 } from '@/lib/format'
import { cn } from '@/lib/cn'

type ToolState = 'pending' | 'running' | 'success' | 'error'

export function ToolCallCard(props: { message: Message }) {
  const [open, setOpen] = useState(false)
  const meta = (props.message.metadata ?? {}) as {
    state?: ToolState
    name?: string
    args?: unknown
    result?: unknown
    duration_ms?: number
  }
  const state: ToolState = meta.state ?? 'success'
  const name = meta.name ?? 'tool'
  const summary = useMemo(() => JSON.stringify(meta.args ?? {}).slice(0, 120), [meta.args])
  const dur = formatDurationMs(meta.duration_ms)

  useEffect(() => {
    const collapse = () => setOpen(false)
    window.addEventListener('mp:collapse-toolcalls', collapse)
    return () => window.removeEventListener('mp:collapse-toolcalls', collapse)
  }, [])

  useEffect(() => {
    if (state === 'error') setOpen(true)
  }, [state])

  const { text: resText, truncated } = truncateUtf8(
    typeof meta.result === 'string' ? meta.result : JSON.stringify(meta.result ?? ''),
    2048,
  )

  return (
    <button
      type="button"
      className={cn(
        'w-full border border-border bg-canvas text-left font-mono text-xs text-fg transition-colors duration-[60ms]',
        state === 'pending' && 'border-dashed',
        state === 'running' && 'animate-pulse-border',
        state === 'error' && 'border-error',
        'focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent',
      )}
      onClick={() => setOpen((o) => !o)}
    >
      <div className="flex items-center justify-between px-3 py-2">
        <span>
          ▸ {name}({summary}) · {dur}
        </span>
        <span className="text-fg-faint">{open ? '▾' : '▸'}</span>
      </div>
      {open ? (
        <div className="space-y-2 border-t border-border px-3 py-2 text-fg-dim">
          <pre className="overflow-auto whitespace-pre-wrap font-code text-[11px]">
            {JSON.stringify(meta.args, null, 2)}
          </pre>
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap font-code text-[11px] text-fg">
            {resText}
            {truncated ? '…' : ''}
          </pre>
          {truncated ? <span className="text-accent">[expand]</span> : null}
        </div>
      ) : null}
    </button>
  )
}
