import { Button } from '@/components/ui/button'

export function TopBar(props: { onOpenPalette: () => void; onOpenSettings: () => void }) {
  return (
    <header className="flex items-center justify-between border-b border-border px-6 py-3">
      <div className="font-mono text-4xl font-bold uppercase tracking-[0.05em] text-fg">
        MONEYPENNY
      </div>
      <div className="flex items-center gap-3">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="font-mono text-xs uppercase tracking-wide text-fg-dim hover:text-fg"
          onClick={props.onOpenPalette}
        >
          ⌘K
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="font-mono text-xs uppercase tracking-wide text-fg-dim hover:text-fg"
          onClick={props.onOpenSettings}
        >
          settings
        </Button>
      </div>
    </header>
  )
}
