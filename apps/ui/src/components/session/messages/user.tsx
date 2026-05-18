import { memo } from 'react'
import { cn } from '@/lib/cn'

export const UserMessage = memo(function UserMessage(props: { content: string }) {
  return (
    <div
      className={cn(
        'border-l-2 border-accent bg-panel px-3 py-2 font-mono text-sm text-fg',
      )}
    >
      {props.content}
    </div>
  )
})
