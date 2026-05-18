import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
} from '@tanstack/react-table'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { IdeaDetailDrawer } from '@/components/ideas/detail-drawer'
import { IdeaEditor } from '@/components/ideas/editor'
import { NewSessionDialog } from '@/components/layout/new-session-dialog'
import type { Idea } from '@/lib/api-types'
import { useTabs } from '@/hooks/use-tabs'
import { formatRelativeTime } from '@/lib/format'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'

function linkedLabel(i: Idea): string {
  const n = (i.frontmatter.impl_session_ids?.length ?? 0) + (i.frontmatter.spec_session_id ? 1 : 0)
  return String(n)
}

export function IdeasTable() {
  const [detail, setDetail] = useState<Idea | null>(null)
  const [editorOpen, setEditorOpen] = useState(false)
  const [launchIdea, setLaunchIdea] = useState<Idea | null>(null)
  const navigate = useNavigate()
  const { openTab } = useTabs()
  const q = useQuery({ queryKey: queryKeys.ideas({}), queryFn: () => api.ideas.list({}) })

  const columns = useMemo<ColumnDef<Idea>[]>(
    () => [
      {
        header: 'TITLE',
        accessorFn: (r) => r.frontmatter.title ?? r.filename,
      },
      { header: 'STATUS', accessorFn: (r) => r.frontmatter.status ?? '—' },
      { header: 'PRIORITY', accessorFn: (r) => r.frontmatter.priority ?? '—' },
      {
        header: 'TAGS',
        accessorFn: (r) => (r.frontmatter.tags ?? []).join(', '),
      },
      {
        header: 'LINKED',
        accessorFn: linkedLabel,
      },
      {
        header: 'UPDATED',
        accessorFn: (r) => r.frontmatter.updated_at ?? '',
        cell: (c) => {
          const v = c.getValue<string>()
          if (!v) return '—'
          return formatRelativeTime(new Date(v).getTime())
        },
      },
      {
        id: 'actions',
        header: '',
        cell: (c) => (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="font-mono text-xs uppercase text-accent"
            onClick={(e) => {
              e.stopPropagation()
              setLaunchIdea(c.row.original)
            }}
          >
            launch
          </Button>
        ),
      },
    ],
    [],
  )

  const data = q.data?.items ?? []
  const table = useReactTable({ data, columns, getCoreRowModel: getCoreRowModel() })

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="font-mono text-2xl font-semibold uppercase tracking-wide text-fg">ideas</h1>
        <Button variant="primary" size="sm" className="uppercase" onClick={() => setEditorOpen(true)}>
          new idea
        </Button>
      </div>
      <div className="border border-border bg-panel">
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
                  const idea = row.original
                  const fresh = await api.ideas.get(idea.filename)
                  setDetail(fresh)
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
      <IdeaDetailDrawer idea={detail} onOpenChange={(o) => !o && setDetail(null)} />
      <IdeaEditor open={editorOpen} onOpenChange={setEditorOpen} />
      <NewSessionDialog
        open={launchIdea !== null}
        onOpenChange={(open) => { if (!open) setLaunchIdea(null) }}
        linkedIdeaFilename={launchIdea?.filename}
        onLaunched={async (sessionId) => {
          await openTab({ kind: 'session', session_id: sessionId, label: launchIdea?.frontmatter.title ?? 'session' })
          setLaunchIdea(null)
          navigate({ to: '/s/$sessionId', params: { sessionId } })
        }}
      />
    </div>
  )
}
