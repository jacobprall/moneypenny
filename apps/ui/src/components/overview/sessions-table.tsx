import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
} from '@tanstack/react-table'
import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useNavigate } from '@tanstack/react-router'
import type { Session } from '@/lib/api-types'
import { formatRelativeTime, formatUsd } from '@/lib/format'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'
import { useTabs } from '@/hooks/use-tabs'
import { cn } from '@/lib/cn'

function glyph(status: string | undefined): string {
  if (status === 'active' || status === 'running') return '▶'
  if (status === 'paused') return '⏸'
  if (status === 'completed') return '✓'
  if (status === 'failed') return '!'
  if (status === 'archived') return '·'
  return '●'
}

export function SessionsTable() {
  const navigate = useNavigate()
  const { openTab } = useTabs()
  const q = useQuery({
    queryKey: queryKeys.sessions({ status: 'active' }),
    queryFn: () => api.sessions.list({ status: 'active', limit: 100 }),
  })

  const columns = useMemo<ColumnDef<Session>[]>(
    () => [
      { header: 'LABEL', accessorKey: 'label' },
      {
        header: 'STATUS',
        accessorKey: 'status',
        cell: (ctx) => {
          const s = ctx.getValue<string>()
          return (
            <span className={cn(s === 'failed' && 'text-error', s === 'paused' && 'text-warn')}>
              {glyph(s)} {s}
            </span>
          )
        },
      },
      { header: 'BLUEPRINT', accessorKey: 'blueprint' },
      {
        header: 'LAST',
        accessorKey: 'last_activity_at',
        cell: (ctx) => formatRelativeTime(ctx.getValue<number>()),
      },
      {
        header: 'COST',
        accessorKey: 'cost_usd',
        cell: (ctx) => formatUsd(ctx.getValue<number>()),
      },
    ],
    [],
  )

  const data = q.data?.items ?? []
  const table = useReactTable({ data, columns, getCoreRowModel: getCoreRowModel() })

  if (!q.isLoading && data.length === 0) {
    return (
      <div className="border border-dashed border-border p-8 font-mono text-sm text-fg-dim">
        <div className="text-base font-semibold uppercase tracking-wide text-fg">NO ACTIVE SESSIONS</div>
        <div className="mt-2">press cmd-n to start one</div>
      </div>
    )
  }

  return (
    <div className="border border-border bg-panel">
      <div className="border-b border-border px-4 py-2 font-mono text-xs font-semibold uppercase tracking-wide text-fg">
        active sessions
      </div>
      <table className="w-full border-collapse text-left font-mono text-sm">
        <thead>
          {table.getHeaderGroups().map((hg) => (
            <tr key={hg.id} className="border-b border-border">
              {hg.headers.map((h) => (
                <th key={h.id} className="px-3 py-2 text-xs font-semibold uppercase tracking-wide text-fg-dim">
                  {flexRender(h.column.columnDef.header, h.getContext())}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody>
          {table.getRowModel().rows.map((row) => (
            <tr
              key={row.id}
              className="cursor-pointer border-b border-border hover:bg-panel-active"
              onClick={async () => {
                const s = row.original
                await openTab({ kind: 'session', session_id: s.id, label: s.label ?? s.id })
                navigate({ to: '/s/$sessionId', params: { sessionId: s.id } })
              }}
            >
              {row.getVisibleCells().map((cell) => (
                <td key={cell.id} className="px-3 py-2 text-fg">
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
