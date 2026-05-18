import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Message } from '@/lib/api-types'
import { AssistantMessage } from '@/components/session/messages/assistant'
import { ChildCard } from '@/components/session/messages/child-card'
import { FileDiffMessage } from '@/components/session/messages/file-diff'
import { TerminalOutput } from '@/components/session/messages/terminal-output'
import { ToolCallCard } from '@/components/session/messages/tool-call'
import { UserMessage } from '@/components/session/messages/user'

export function MessageList(props: { messages: Message[]; sessionId: string }) {
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const [pinned, setPinned] = useState(true)
  const [newSinceScroll, setNewSinceScroll] = useState(false)
  const lastLen = useRef(props.messages.length)

  useEffect(() => {
    if (props.messages.length > lastLen.current && !pinned) {
      setNewSinceScroll(true)
    }
    lastLen.current = props.messages.length
  }, [props.messages.length, pinned])

  const onScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80
    setPinned(nearBottom)
    if (nearBottom) setNewSinceScroll(false)
  }, [])

  useEffect(() => {
    const el = scrollRef.current
    if (!el || !pinned) return
    el.scrollTop = el.scrollHeight
  }, [props.messages, pinned])

  useEffect(() => {
    const nav = (ev: Event) => {
      const d = (ev as CustomEvent<{ dir: number }>).detail?.dir
      if (!d) return
      /* minimal: scroll by chunk */
      scrollRef.current?.scrollBy({ top: d * 48, behavior: 'smooth' })
    }
    window.addEventListener('mp:message-nav', nav as EventListener)
    return () => window.removeEventListener('mp:message-nav', nav as EventListener)
  }, [])

  const rendered = useMemo(
    () =>
      props.messages.map((m) => (
        <MessageSwitch key={m.id} message={m} sessionId={props.sessionId} />
      )),
    [props.messages, props.sessionId],
  )

  return (
    <div className="relative h-full min-h-0">
      <div
        id="conversation-scroll"
        ref={scrollRef}
        onScroll={onScroll}
        className="h-full overflow-y-auto px-4 py-3"
      >
        <div className="flex flex-col gap-3">{rendered}</div>
      </div>
      {newSinceScroll ? (
        <button
          type="button"
          className="absolute bottom-4 right-4 border border-accent bg-panel px-3 py-1 font-mono text-xs uppercase text-accent focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
          onClick={() => {
            const el = scrollRef.current
            if (el) el.scrollTop = el.scrollHeight
            setPinned(true)
            setNewSinceScroll(false)
          }}
        >
          jump to latest
        </button>
      ) : null}
    </div>
  )
}

function MessageSwitch(props: { message: Message; sessionId: string }) {
  const k = props.message.kind ?? props.message.role
  const meta = (props.message.metadata ?? {}) as Record<string, unknown>
  if (k === 'tool' || meta.type === 'tool') {
    return <ToolCallCard message={props.message} />
  }
  if (meta.type === 'diff' || k === 'diff') {
    return <FileDiffMessage text={props.message.content} />
  }
  if (meta.type === 'terminal' || k === 'terminal') {
    return <TerminalOutput text={props.message.content} />
  }
  if (meta.type === 'child' || k === 'child') {
    return <ChildCard meta={meta} />
  }
  if (props.message.role === 'user') {
    return <UserMessage content={props.message.content} />
  }
  return (
    <AssistantMessage
      messageId={props.message.id}
      content={props.message.content}
    />
  )
}
