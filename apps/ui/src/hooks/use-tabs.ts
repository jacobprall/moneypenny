import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'
import type { TabRecord } from '@/lib/api-types'

function sortTabs(items: TabRecord[]): TabRecord[] {
  return [...items].sort((a, b) => a.position - b.position)
}

export function useTabs() {
  const qc = useQueryClient()
  const q = useQuery({
    queryKey: queryKeys.tabs(),
    queryFn: async () => sortTabs((await api.tabs.list()).items),
  })

  const invalidate = () => void qc.invalidateQueries({ queryKey: queryKeys.tabs() })

  const openTab = useMutation({
    mutationFn: api.tabs.open,
    onMutate: async (input) => {
      await qc.cancelQueries({ queryKey: queryKeys.tabs() })
      const prev = qc.getQueryData<TabRecord[]>(queryKeys.tabs()) ?? []
      const optimistic: TabRecord = {
        id: `tmp-${typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : String(Date.now())}`,
        kind: input.kind,
        session_id: input.session_id ?? null,
        label: input.label,
        position: prev.length,
        active: true,
      }
      const cleared = prev.map((t) => ({ ...t, active: false }))
      qc.setQueryData<TabRecord[]>(queryKeys.tabs(), sortTabs([...cleared, optimistic]))
      return { prev }
    },
    onError: (_e, _v, ctx) => {
      if (ctx?.prev) qc.setQueryData(queryKeys.tabs(), ctx.prev)
    },
    onSettled: invalidate,
  })

  const closeTab = useMutation({
    mutationFn: api.tabs.close,
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: queryKeys.tabs() })
      const prev = qc.getQueryData<TabRecord[]>(queryKeys.tabs()) ?? []
      qc.setQueryData<TabRecord[]>(
        queryKeys.tabs(),
        prev.filter((t) => t.id !== id),
      )
      return { prev }
    },
    onError: (_e, _v, ctx) => {
      if (ctx?.prev) qc.setQueryData(queryKeys.tabs(), ctx.prev)
    },
    onSettled: invalidate,
  })

  const reorderTabs = useMutation({
    mutationFn: async (ordered: TabRecord[]) => {
      await Promise.all(
        ordered.map((t, idx) => api.tabs.patch(t.id, { position: idx, active: t.active })),
      )
      return ordered
    },
    onMutate: async (ordered) => {
      await qc.cancelQueries({ queryKey: queryKeys.tabs() })
      const prev = qc.getQueryData<TabRecord[]>(queryKeys.tabs())
      qc.setQueryData(queryKeys.tabs(), ordered)
      return { prev }
    },
    onError: (_e, _v, ctx) => {
      if (ctx?.prev) qc.setQueryData(queryKeys.tabs(), ctx.prev)
    },
    onSettled: invalidate,
  })

  const setActiveTab = useMutation({
    mutationFn: async (id: string) => {
      const items = qc.getQueryData<TabRecord[]>(queryKeys.tabs()) ?? []
      const next = items.map((t) => ({ ...t, active: t.id === id }))
      await api.tabs.patch(id, { active: true })
      return next
    },
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: queryKeys.tabs() })
      const prev = qc.getQueryData<TabRecord[]>(queryKeys.tabs()) ?? []
      const next = prev.map((t) => ({ ...t, active: t.id === id }))
      qc.setQueryData(queryKeys.tabs(), sortTabs(next))
      return { prev }
    },
    onError: (_e, _v, ctx) => {
      if (ctx?.prev) qc.setQueryData(queryKeys.tabs(), ctx.prev)
    },
    onSettled: invalidate,
  })

  return {
    tabs: q.data ?? [],
    isLoading: q.isLoading,
    openTab: openTab.mutateAsync,
    closeTab: closeTab.mutateAsync,
    reorderTabs: reorderTabs.mutateAsync,
    setActiveTab: setActiveTab.mutateAsync,
  }
}
