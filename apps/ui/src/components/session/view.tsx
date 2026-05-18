import { useQuery } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { SessionFooter } from '@/components/session/footer'
import { SessionInputBox } from '@/components/session/input-box'
import { MessageList } from '@/components/session/messages/list'
import { PauseNotice } from '@/components/session/messages/pause-notice'
import { useSessionStream } from '@/hooks/use-session-stream'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'

export function SessionView(props: { sessionId: string }) {
  const inputRef = useRef<HTMLTextAreaElement | null>(null)

  useEffect(() => {
    const fn = () => inputRef.current?.focus()
    window.addEventListener('mp:focus-session-input', fn)
    return () => window.removeEventListener('mp:focus-session-input', fn)
  }, [])

  const sessionQ = useQuery({
    queryKey: queryKeys.session(props.sessionId),
    queryFn: () => api.sessions.get(props.sessionId),
  })
  const stream = useSessionStream(props.sessionId)
  const s = sessionQ.data

  if (sessionQ.isLoading) {
    return (
      <div className="flex flex-1 animate-pulse-border items-center justify-center border border-border p-8 font-mono text-fg-dim">
        loading session…
      </div>
    )
  }

  if (!s) {
    return (
      <div className="p-6 font-mono text-error">
        ! session not found
      </div>
    )
  }

  const blocked = s.status === 'failed' || s.status === 'archived'

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-hidden">
        {stream.pause ? (
          <PauseNotice
            sessionId={props.sessionId}
            reason={stream.pause.reason}
            options={stream.pause.options}
          />
        ) : null}
        <MessageList messages={stream.messages} sessionId={props.sessionId} />
      </div>
      <SessionFooter session={s} costUsd={s.cost_usd} />
      <SessionInputBox
        ref={inputRef}
        sessionId={props.sessionId}
        disabled={blocked}
      />
    </div>
  )
}
