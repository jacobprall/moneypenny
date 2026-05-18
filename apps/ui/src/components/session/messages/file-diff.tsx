import { useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/cn'

export function FileDiffMessage(props: { text: string }) {
  const lines = props.text.split('\n')
  const collapse = lines.length > 50
  const [open, setOpen] = useState(!collapse)
  const visible = useMemo(() => (open ? lines : lines.slice(0, 50)), [collapse, lines, open])

  return (
    <div className="border border-border bg-canvas font-code text-xs">
      <div className="max-h-96 overflow-auto">
        {visible.map((line, i) => {
          const add = line.startsWith('+')
          const del = line.startsWith('-')
          return (
            <div
              key={i}
              className={cn(
                'flex gap-2 px-2 py-0.5',
                add && 'text-success',
                del && 'text-error',
                !add && !del && 'text-fg-dim',
              )}
            >
              <span className="w-8 shrink-0 text-right text-fg-faint">{i + 1}</span>
              <span className="whitespace-pre-wrap">{line}</span>
            </div>
          )
        })}
      </div>
      {collapse ? (
        <div className="border-t border-border p-2">
          <Button type="button" variant="ghost" size="sm" onClick={() => setOpen((o) => !o)}>
            {open ? 'collapse' : 'expand'}
          </Button>
        </div>
      ) : null}
    </div>
  )
}
