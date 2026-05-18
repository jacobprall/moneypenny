import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { SseManager } from '@/lib/sse'
import type { ServerEvent, SessionStatus } from '@/lib/api-types'
import { queryKeys } from '@/lib/query-keys'

export interface GlobalEventsState {
  recent: ServerEvent[]
  statusBySession: Record<string, SessionStatus>
}

const MAX = 50

export function useGlobalEvents(): GlobalEventsState {
  const qc = useQueryClient()
  const [recent, setRecent] = useState<ServerEvent[]>([])
  const [statusBySession, setStatusBySession] = useState<Record<string, SessionStatus>>({})
  const recentRef = useRef<ServerEvent[]>([])

  useEffect(() => {
    const sub = SseManager.get().subscribe('global', (ev) => {
      recentRef.current = [...recentRef.current, ev].slice(-MAX)
      setRecent(recentRef.current)

      // --- session status tracking ---
      if (ev.type === 'session.status_changed' && ev.session_id) {
        const to = ev.detail.to
        if (typeof to === 'string') {
          setStatusBySession((prev) => ({ ...prev, [ev.session_id as string]: to }))
        }
        void qc.invalidateQueries({ queryKey: queryKeys.sessions({}) })
      }

      // --- tab changes ---
      if (
        ev.type === 'tab.opened' ||
        ev.type === 'tab.closed' ||
        ev.type === 'session.created'
      ) {
        void qc.invalidateQueries({ queryKey: queryKeys.tabs() })
      }

      // --- session lifecycle ---
      if (
        ev.type === 'session.completed' ||
        ev.type === 'session.failed' ||
        ev.type === 'session.archived' ||
        ev.type === 'session.deleted'
      ) {
        void qc.invalidateQueries({ queryKey: queryKeys.sessions({}) })
        void qc.invalidateQueries({ queryKey: queryKeys.tabs() })
      }

      // --- run lifecycle (refresh overview tables) ---
      if (
        ev.type === 'run.started' ||
        ev.type === 'run.completed' ||
        ev.type === 'run.failed' ||
        ev.type === 'run.aborted'
      ) {
        void qc.invalidateQueries({ queryKey: queryKeys.sessions({}) })
        void qc.invalidateQueries({ queryKey: queryKeys.health() })
      }

      // --- knowledge events ---
      if (ev.type.startsWith('knowledge.')) {
        void qc.invalidateQueries({ queryKey: ['knowledge'] })
      }

      // --- blueprint events ---
      if (ev.type.startsWith('blueprint.')) {
        void qc.invalidateQueries({ queryKey: queryKeys.blueprints() })
      }

      // --- schedule events ---
      if (ev.type.startsWith('schedule.')) {
        void qc.invalidateQueries({ queryKey: queryKeys.sessions({}) })
      }

      // --- system events ---
      if (ev.type === 'system.started' || ev.type === 'system.shutdown') {
        void qc.invalidateQueries({ queryKey: queryKeys.health() })
      }

      // --- index completed ---
      if (ev.type === 'index.completed') {
        void qc.invalidateQueries({ queryKey: queryKeys.health() })
      }

      // --- budget events ---
      if (ev.type === 'budget.warned' || ev.type === 'budget.exceeded') {
        void qc.invalidateQueries({ queryKey: queryKeys.health() })
      }
    })
    return sub
  }, [qc])

  return { recent, statusBySession }
}
