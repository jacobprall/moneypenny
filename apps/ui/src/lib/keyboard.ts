export type ShortcutHandler = (ev: KeyboardEvent) => void

export interface ShortcutSpec {
  id: string
  key: string
  /** when true, cmd on mac, ctrl on win/linux */
  meta?: boolean
  shift?: boolean
  /** match when target is input/textarea/contenteditable */
  allowInField?: boolean
  handler: ShortcutHandler
}

function isFieldTarget(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false
  const tag = target.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
  return target.isContentEditable
}

function metaPressed(ev: KeyboardEvent): boolean {
  return ev.metaKey || ev.ctrlKey
}

export function bindShortcuts(specs: ShortcutSpec[]): () => void {
  const onKeyDown = (ev: KeyboardEvent) => {
    if (ev.defaultPrevented) return
    for (const spec of specs) {
      if (spec.key.length === 1 && spec.key !== ev.key.toLowerCase() && spec.key !== ev.key) continue
      if (spec.key.length > 1 && spec.key !== ev.key) continue
      if (spec.meta && !metaPressed(ev)) continue
      if (!spec.meta && (ev.metaKey || ev.ctrlKey)) continue
      if (spec.shift !== undefined && spec.shift !== ev.shiftKey) continue
      if (!spec.allowInField && isFieldTarget(ev.target)) continue
      ev.preventDefault()
      spec.handler(ev)
      break
    }
  }
  window.addEventListener('keydown', onKeyDown)
  return () => window.removeEventListener('keydown', onKeyDown)
}

export function normalizeKey(k: string): string {
  return k.length === 1 ? k.toLowerCase() : k
}
