import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useEffect, useMemo, useRef, useState } from 'react'
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandPaletteShell,
} from '@/components/ui/command'
import type { BlueprintMeta, Idea, Session, TabRecord } from '@/lib/api-types'
import { useTabs } from '@/hooks/use-tabs'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'

export function CommandPalette(props: {
  open: boolean
  onOpenChange: (v: boolean) => void
  tabs: TabRecord[]
  onSelectTab: (t: TabRecord) => void
  onNewSession: () => void
}) {
  const [q, setQ] = useState('')
  const [codeHits, setCodeHits] = useState<unknown[]>([])
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const navigate = useNavigate()
  const { openTab } = useTabs()

  const sessionsQ = useQuery({
    queryKey: queryKeys.sessions({}),
    queryFn: () => api.sessions.list({ limit: 20 }),
    enabled: props.open,
  })
  const ideasQ = useQuery({
    queryKey: queryKeys.ideas({}),
    queryFn: () => api.ideas.list({}),
    enabled: props.open,
  })
  const bpQ = useQuery({
    queryKey: queryKeys.blueprints(),
    queryFn: () => api.blueprints.list(),
    enabled: props.open,
  })

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => {
      const s = q.trim()
      if (s.length < 2) {
        setCodeHits([])
        return
      }
      void api.code.search(s).then((r) => setCodeHits(r.items))
    }, 280)
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current)
    }
  }, [q])

  const sessions = sessionsQ.data?.items ?? []
  const ideas = ideasQ.data?.items ?? []
  const bps = bpQ.data?.items ?? []

  const filteredSessions = useMemo(() => {
    const s = q.trim().toLowerCase()
    if (!s) return sessions.slice(0, 8)
    return sessions.filter((x) => x.label?.toLowerCase().includes(s) ?? false).slice(0, 8)
  }, [q, sessions])

  const filteredIdeas = useMemo(() => {
    const s = q.trim().toLowerCase()
    if (!s) return ideas.slice(0, 8)
    return ideas.filter((i) => i.frontmatter.title?.toLowerCase().includes(s)).slice(0, 8)
  }, [q, ideas])

  const filteredBp = useMemo(() => {
    const s = q.trim().toLowerCase()
    if (!s) return bps.slice(0, 8)
    return bps.filter((b) => b.name.toLowerCase().includes(s)).slice(0, 8)
  }, [q, bps])

  const navigateToSession = async (s: Session) => {
    await openTab({ kind: 'session', session_id: s.id, label: s.label ?? s.id })
    navigate({ to: '/s/$sessionId', params: { sessionId: s.id } })
    props.onOpenChange(false)
  }

  const navigateToSearch = () => {
    navigate({ to: '/search' })
    props.onOpenChange(false)
  }

  const navigateToIdeas = () => {
    navigate({ to: '/ideas' })
    props.onOpenChange(false)
  }

  return (
    <CommandPaletteShell open={props.open} onOpenChange={props.onOpenChange}>
      <Command
        className="border-0"
        loop
        shouldFilter={false}
        onKeyDown={(e) => {
          if (e.key === 'Escape') props.onOpenChange(false)
        }}
      >
        <CommandInput
          placeholder="type a command or search…"
          value={q}
          onValueChange={setQ}
        />
        <CommandList>
          <CommandEmpty>no results</CommandEmpty>
          <CommandGroup heading="actions">
            <CommandItem
              onSelect={() => {
                props.onOpenChange(false)
                props.onNewSession()
              }}
            >
              new session ⌘N
            </CommandItem>
            <CommandItem onSelect={navigateToSearch}>
              search
            </CommandItem>
            <CommandItem onSelect={navigateToIdeas}>
              ideas
            </CommandItem>
            <CommandItem
              onSelect={() => {
                void api.code.index()
                props.onOpenChange(false)
              }}
            >
              reindex code
            </CommandItem>
            <CommandItem
              onSelect={async () => {
                const items = (await api.sessions.list({ status: 'active' })).items
                await Promise.all(items.map((s) => api.sessions.pause(s.id)))
                props.onOpenChange(false)
              }}
            >
              pause all running
            </CommandItem>
          </CommandGroup>
          <CommandGroup heading="open tabs">
            {props.tabs.map((t) => (
              <CommandItem
                key={t.id}
                value={`tab-${t.id}`}
                onSelect={() => {
                  props.onSelectTab(t)
                  props.onOpenChange(false)
                }}
              >
                {t.label}
              </CommandItem>
            ))}
          </CommandGroup>
          <CommandGroup heading="recent sessions">
            {filteredSessions.map((s: Session) => (
              <CommandItem
                key={s.id}
                value={`session-${s.id}`}
                onSelect={() => void navigateToSession(s)}
              >
                <span className="flex-1 truncate">{s.label ?? s.id}</span>
                <span className="text-fg-faint">{s.status}</span>
              </CommandItem>
            ))}
          </CommandGroup>
          <CommandGroup heading="ideas">
            {filteredIdeas.map((i: Idea) => (
              <CommandItem key={i.filename} value={`idea-${i.filename}`} onSelect={navigateToIdeas}>
                {i.frontmatter.title ?? i.filename}
              </CommandItem>
            ))}
          </CommandGroup>
          <CommandGroup heading="blueprints">
            {filteredBp.map((b: BlueprintMeta) => (
              <CommandItem key={b.name} value={`bp-${b.name}`}>
                {b.name}
              </CommandItem>
            ))}
          </CommandGroup>
          {codeHits.length > 0 && (
            <CommandGroup heading="code">
              {codeHits.slice(0, 12).map((h, idx) => (
                <CommandItem key={idx} value={`code-${idx}`}>
                  {JSON.stringify(h)}
                </CommandItem>
              ))}
            </CommandGroup>
          )}
        </CommandList>
      </Command>
    </CommandPaletteShell>
  )
}
