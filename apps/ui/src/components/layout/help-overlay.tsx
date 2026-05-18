import { CommandPaletteShell } from '@/components/ui/command'

const ROWS: [string, string][] = [
  ['⌘K', 'command palette'],
  ['⌘N', 'new session'],
  ['⌘P', 'quick-switch tabs'],
  ['⌘W', 'close active tab'],
  ['⌘[ / ⌘]', 'prev / next tab'],
  ['⌘1–⌘9', 'jump to tab'],
  ['⌘E', 'focus input'],
  ['⌘/', 'this help overlay'],
  ['⌘.', 'pause running'],
  ['Esc', 'collapse tool calls'],
  ['j / k', 'message navigation'],
  ['g g', 'scroll conversation top'],
  ['G', 'scroll conversation bottom'],
]

export function HelpOverlay(props: { open: boolean; onOpenChange: (v: boolean) => void }) {
  return (
    <CommandPaletteShell open={props.open} onOpenChange={props.onOpenChange}>
      <div className="border-b border-border px-4 py-3 font-mono text-lg font-semibold uppercase tracking-wide text-fg">
        keyboard
      </div>
      <div className="overflow-auto p-4 font-mono text-sm">
        <table className="w-full border-collapse text-left">
          <tbody>
            {ROWS.map(([k, v]) => (
              <tr key={k} className="border-b border-border">
                <th className="py-2 pr-4 text-xs font-semibold uppercase tracking-wide text-accent">
                  {k}
                </th>
                <td className="py-2 text-fg-dim">{v}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </CommandPaletteShell>
  )
}
