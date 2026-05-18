import { Button } from '@/components/ui/button'
import { api } from '@/lib/rpc'

export function PauseNotice(props: { sessionId: string; reason: string; options?: string[] }) {
  return (
    <div className="border-b border-warn bg-panel px-4 py-3 font-mono text-sm text-warn">
      <div className="mb-2">
        ⏸ paused · {props.reason} · type to resume
      </div>
      {props.options?.length ? (
        <div className="flex flex-wrap gap-2">
          {props.options.map((o) => (
            <Button
              key={o}
              type="button"
              variant="ghost"
              size="sm"
              className="border border-warn text-warn"
              onClick={() => void api.sessions.inject(props.sessionId, { content: o })}
            >
              {o}
            </Button>
          ))}
        </div>
      ) : null}
    </div>
  )
}
