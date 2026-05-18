import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { SseManager } from '@/lib/sse'
import type { Message } from '@/lib/api-types'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'

export interface RunUiState {
  id: string | null
  status: 'idle' | 'running' | 'completed' | 'failed'
}

export interface ToolUiState {
  id: string
  name: string
  status: 'running' | 'completed' | 'error'
  error?: string
}

export interface ChildSpawn {
  childSessionId: string
  label?: string
}

export interface SessionStreamState {
  messages: Message[]
  run: RunUiState
  tabStatus: 'running' | 'paused' | 'completed' | 'failed' | 'idle' | 'archived'
  pause?: { reason: string; options?: string[] }
  streaming: Record<string, string>
  tools: Record<string, ToolUiState>
  children: ChildSpawn[]
}

function mergeType(t: string): string {
  switch (t) {
    case 'message.started':
      return 'message.assistant.started'
    case 'message.token':
      return 'message.assistant.token'
    case 'message.completed':
      return 'message.assistant.completed'
    case 'status.changed':
      return 'session.status_changed'
    case 'config.changed':
      return 'session.config_changed'
    default:
      return t
  }
}

export function useSessionStream(sessionId: string) {
  const qc = useQueryClient()
  const msgsQ = useQuery({
    queryKey: queryKeys.sessionMessages(sessionId),
    queryFn: async () =>
      (
        await api.messages.listBySession(sessionId, {
          limit: 80,
          direction: 'before',
        })
      ).items,
  })

  const [streaming, setStreaming] = useState<Record<string, string>>({})
  const [run, setRun] = useState<RunUiState>({ id: null, status: 'idle' })
  const [tabStatus, setTabStatus] = useState<SessionStreamState['tabStatus']>('idle')
  const [pause, setPause] = useState<SessionStreamState['pause']>(undefined)
  const [tools, setTools] = useState<Record<string, ToolUiState>>({})
  const [children, setChildren] = useState<ChildSpawn[]>([])

  const appendToken = useCallback((messageId: string, chunk: string) => {
    setStreaming((prev) => {
      const next = { ...prev, [messageId]: (prev[messageId] ?? '') + chunk }
      return next
    })
  }, [])

  const finalizeMessage = useCallback(
    (messageId: string) => {
      setStreaming((prev) => {
        const next = { ...prev }
        delete next[messageId]
        return next
      })
      void qc.invalidateQueries({ queryKey: queryKeys.sessionMessages(sessionId) })
    },
    [qc, sessionId],
  )

  useEffect(() => {
    const ch = `session:${sessionId}` as const
    const unsub = SseManager.get().subscribe(ch, (ev) => {
      const type = mergeType(ev.type)

      // --- run lifecycle ---
      if (type === 'run.started') {
        const rid = typeof ev.detail.run_id === 'string' ? ev.detail.run_id : null
        setRun({ id: rid, status: 'running' })
        setTabStatus('running')
        return
      }
      if (type === 'run.completed') {
        setRun((r) => ({ ...r, status: 'completed' }))
        void qc.invalidateQueries({ queryKey: queryKeys.session(sessionId) })
        return
      }
      if (type === 'run.failed') {
        setRun((r) => ({ ...r, status: 'failed' }))
        setTabStatus('failed')
        void qc.invalidateQueries({ queryKey: queryKeys.session(sessionId) })
        return
      }
      if (type === 'run.aborted') {
        setRun((r) => ({ ...r, status: 'failed' }))
        void qc.invalidateQueries({ queryKey: queryKeys.session(sessionId) })
        return
      }

      // --- message streaming ---
      if (type === 'message.assistant.started') {
        void qc.invalidateQueries({ queryKey: queryKeys.sessionMessages(sessionId) })
        return
      }
      if (type === 'message.assistant.token') {
        const mid = ev.detail.message_id
        const content = ev.detail.content
        if (typeof mid === 'string' && typeof content === 'string') appendToken(mid, content)
        return
      }
      if (type === 'message.assistant.completed') {
        const mid = ev.detail.message_id
        if (typeof mid === 'string') finalizeMessage(mid)
        return
      }

      // --- tool lifecycle ---
      if (type === 'tool.started') {
        const toolId = String(ev.detail.tool_call_id ?? ev.detail.id ?? '')
        const name = String(ev.detail.name ?? 'tool')
        if (toolId) {
          setTools((prev) => ({ ...prev, [toolId]: { id: toolId, name, status: 'running' } }))
        }
        void qc.invalidateQueries({ queryKey: queryKeys.sessionMessages(sessionId) })
        return
      }
      if (type === 'tool.completed') {
        const toolId = String(ev.detail.tool_call_id ?? ev.detail.id ?? '')
        if (toolId) {
          setTools((prev) => ({ ...prev, [toolId]: { ...prev[toolId], id: toolId, name: prev[toolId]?.name ?? 'tool', status: 'completed' } }))
        }
        void qc.invalidateQueries({ queryKey: queryKeys.sessionMessages(sessionId) })
        return
      }
      if (type === 'tool.failed') {
        const toolId = String(ev.detail.tool_call_id ?? ev.detail.id ?? '')
        const error = typeof ev.detail.error === 'string' ? ev.detail.error : undefined
        if (toolId) {
          setTools((prev) => ({ ...prev, [toolId]: { ...prev[toolId], id: toolId, name: prev[toolId]?.name ?? 'tool', status: 'error', error } }))
        }
        void qc.invalidateQueries({ queryKey: queryKeys.sessionMessages(sessionId) })
        return
      }

      // --- session status ---
      if (type === 'session.status_changed') {
        const to = ev.detail.to
        if (to === 'paused') {
          setTabStatus('paused')
        } else if (to === 'failed') {
          setTabStatus('failed')
        } else if (to === 'completed') {
          setTabStatus('completed')
        } else if (to === 'archived') {
          setTabStatus('archived')
        } else if (to === 'active' || to === 'running') {
          setTabStatus('running')
        }
        void qc.invalidateQueries({ queryKey: queryKeys.session(sessionId) })
        return
      }
      if (type === 'session.config_changed') {
        void qc.invalidateQueries({ queryKey: queryKeys.session(sessionId) })
        return
      }

      // --- HITL ---
      if (type === 'hitl.requested') {
        const reason = typeof ev.detail.reason === 'string' ? ev.detail.reason : 'human input'
        const options = Array.isArray(ev.detail.options)
          ? ev.detail.options.filter((x): x is string => typeof x === 'string')
          : undefined
        setPause({ reason, options })
        setTabStatus('paused')
        return
      }
      if (type === 'hitl.checkpoint') {
        const reason = typeof ev.detail.reason === 'string' ? ev.detail.reason : 'checkpoint'
        setPause({ reason })
        setTabStatus('paused')
        return
      }
      if (type === 'hitl.resumed') {
        setPause(undefined)
        setTabStatus('running')
        return
      }

      // --- child sessions ---
      if (type === 'child.spawned') {
        const childId = typeof ev.detail.child_session_id === 'string' ? ev.detail.child_session_id : null
        const label = typeof ev.detail.label === 'string' ? ev.detail.label : undefined
        if (childId) {
          setChildren((prev) => [...prev, { childSessionId: childId, label }])
        }
        void qc.invalidateQueries({ queryKey: queryKeys.sessionMessages(sessionId) })
        return
      }
      if (type === 'child.completed' || type === 'child.failed') {
        void qc.invalidateQueries({ queryKey: queryKeys.sessionMessages(sessionId) })
        return
      }

      // --- knowledge events: refresh session data ---
      if (type.startsWith('knowledge.')) {
        void qc.invalidateQueries({ queryKey: queryKeys.session(sessionId) })
        return
      }
    })
    return unsub
  }, [appendToken, finalizeMessage, qc, sessionId])

  const messages = useMemo(() => {
    const base = msgsQ.data ?? []
    return base.map((m) => {
      const s = streaming[m.id]
      if (!s) return m
      return { ...m, content: (m.content ?? '') + s }
    })
  }, [msgsQ.data, streaming])

  return { messages, run, tabStatus, pause, streaming, tools, children, refresh: msgsQ.refetch }
}
