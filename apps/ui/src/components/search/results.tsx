import {
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
} from '@tanstack/react-table'
import { useQuery } from '@tanstack/react-query'
import { useEffect, useMemo, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { Input } from '@/components/ui/input'
import { ResultSection } from '@/components/search/result-section'
import type { CodeSearchHit, Idea, KnowledgeHit, Message, Session } from '@/lib/api-types'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'
import { useTabs } from '@/hooks/use-tabs'

function KnowledgeSection(props: { title: string; items: KnowledgeHit[]; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(props.defaultOpen ?? false)
  return (
    <details open={open} onToggle={(e) => setOpen((e.target as HTMLDetailsElement).open)}>
      <summary className="cursor-pointer px-3 py-2 font-mono text-xs font-semibold uppercase tracking-wide text-fg-dim hover:text-fg">
        {props.title} ({props.items.length})
      </summary>
      <div className="space-y-1 px-3 pb-2">
        {props.items.length === 0 && (
          <p className="font-mono text-xs text-fg-faint">none found</p>
        )}
        {props.items.map((item, idx) => (
          <div key={item.id ?? idx} className="border-b border-border py-1 font-mono text-xs text-fg">
            <span className="text-accent">{item.name ?? item.id}</span>
            {item.kind && <span className="ml-2 text-fg-faint">[{item.kind}]</span>}
          </div>
        ))}
      </div>
    </details>
  )
}

export function SearchResults() {
  const [q, setQ] = useState('')
  const [debounced, setDebounced] = useState('')
  const navigate = useNavigate()
  const { openTab } = useTabs()

  useEffect(() => {
    const t = setTimeout(() => setDebounced(q.trim()), 250)
    return () => clearTimeout(t)
  }, [q])

  const enabled = debounced.length > 1
  const sessionsQ = useQuery({
    queryKey: queryKeys.sessions({ label: debounced }),
    queryFn: () => api.sessions.list({ label: debounced, limit: 30 }),
    enabled,
  })
  const msgsQ = useQuery({
    queryKey: queryKeys.messagesSearch(debounced),
    queryFn: () => api.messages.search(debounced),
    enabled,
  })
  const ideasQ = useQuery({
    queryKey: queryKeys.ideas({}),
    queryFn: () => api.ideas.list({}),
    enabled,
  })
  const codeQ = useQuery({
    queryKey: queryKeys.codeSearch(debounced),
    queryFn: () => api.code.search(debounced),
    enabled,
  })
  const skillsQ = useQuery({
    queryKey: ['knowledge', 'skills'],
    queryFn: async () => (await api.knowledge.skills()).items as KnowledgeHit[],
    enabled,
  })
  const conventionsQ = useQuery({
    queryKey: ['knowledge', 'conventions'],
    queryFn: async () => (await api.knowledge.conventions()).items as KnowledgeHit[],
    enabled,
  })
  const pointersQ = useQuery({
    queryKey: ['knowledge', 'pointers'],
    queryFn: async () => (await api.knowledge.pointers()).items as KnowledgeHit[],
    enabled,
  })

  const ideasFiltered = useMemo(() => {
    const items = ideasQ.data?.items ?? []
    const s = debounced.toLowerCase()
    return items.filter((i) => (i.frontmatter.title ?? i.filename).toLowerCase().includes(s))
  }, [ideasQ.data, debounced])

  const sessionCols = useMemo<ColumnDef<Session>[]>(
    () => [
      { header: 'LABEL', accessorKey: 'label' },
      { header: 'STATUS', accessorKey: 'status' },
      { header: 'BLUEPRINT', accessorKey: 'blueprint' },
    ],
    [],
  )
  const sessionTable = useReactTable({
    data: sessionsQ.data?.items ?? [],
    columns: sessionCols,
    getCoreRowModel: getCoreRowModel(),
  })

  const msgCols = useMemo<ColumnDef<Message>[]>(
    () => [
      { header: 'SESSION', accessorKey: 'session_id' },
      { header: 'ROLE', accessorKey: 'role' },
      { header: 'PREVIEW', accessorFn: (m) => (m.content ?? '').slice(0, 120) },
    ],
    [],
  )
  const msgTable = useReactTable({
    data: msgsQ.data?.items ?? [],
    columns: msgCols,
    getCoreRowModel: getCoreRowModel(),
  })

  const codeCols = useMemo<ColumnDef<CodeSearchHit>[]>(
    () => [
      { header: 'PATH', accessorKey: 'path' },
      { header: 'LINE', accessorKey: 'line' },
      { header: 'SNIPPET', accessorKey: 'snippet' },
    ],
    [],
  )
  const codeTable = useReactTable({
    data: (codeQ.data?.items ?? []) as CodeSearchHit[],
    columns: codeCols,
    getCoreRowModel: getCoreRowModel(),
  })

  const ideaCols = useMemo<ColumnDef<Idea>[]>(
    () => [
      { header: 'TITLE', accessorFn: (r) => r.frontmatter.title ?? r.filename },
      { header: 'STATUS', accessorFn: (r) => r.frontmatter.status ?? '—' },
    ],
    [],
  )
  const ideaTable = useReactTable({
    data: ideasFiltered,
    columns: ideaCols,
    getCoreRowModel: getCoreRowModel(),
  })

  return (
    <div className="space-y-6">
      <h1 className="font-mono text-2xl font-semibold uppercase tracking-wide text-fg">search</h1>
      <Input
        placeholder="search sessions, code, ideas, knowledge…"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        className="max-w-xl"
      />
      <ResultSection
        title="sessions"
        table={sessionTable}
        onRowClick={async (s) => {
          await openTab({ kind: 'session', session_id: s.id, label: s.label ?? s.id })
          navigate({ to: '/s/$sessionId', params: { sessionId: s.id } })
        }}
      />
      <ResultSection title="messages" table={msgTable} />
      <ResultSection title="code" table={codeTable} />
      <ResultSection title="ideas" table={ideaTable} />
      <section>
        <h2 className="mb-2 font-mono text-xs font-semibold uppercase tracking-wide text-fg-dim">knowledge</h2>
        <div className="border border-border bg-panel">
          <KnowledgeSection title="skills" items={skillsQ.data ?? []} defaultOpen />
          <KnowledgeSection title="conventions" items={conventionsQ.data ?? []} />
          <KnowledgeSection title="pointers" items={pointersQ.data ?? []} />
        </div>
      </section>
    </div>
  )
}
