import { useState } from 'react'
import { Button } from '@/components/ui/button'

export function TerminalOutput(props: { text: string }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="border border-border bg-canvas font-code text-xs text-fg-dim">
      <pre className={open ? 'max-h-[32rem] overflow-auto p-2' : 'max-h-96 overflow-auto p-2'}>
        {props.text}
      </pre>
      <div className="border-t border-border p-1">
        <Button type="button" variant="ghost" size="sm" onClick={() => setOpen((o) => !o)}>
          [expand]
        </Button>
      </div>
    </div>
  )
}
