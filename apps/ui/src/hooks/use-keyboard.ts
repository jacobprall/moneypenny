import { useRouterState } from '@tanstack/react-router'
import { useEffect, useRef } from 'react'
import { bindShortcuts, type ShortcutSpec } from '@/lib/keyboard'

export interface UseKeyboardOptions {
  onCommandPalette: () => void
  onQuickSwitch?: () => void
  onNewSession: () => void
  onSettings: () => void
  onHelpToggle: () => void
  closeActiveTab: () => void
  prevTab: () => void
  nextTab: () => void
  jumpTab: (index: number) => void
  focusSessionInput: () => void
  collapseExpanded: () => void
  pauseRunningSessions: () => void
}

export function useKeyboard(opts: UseKeyboardOptions) {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const lastG = useRef(0)
  const optsRef = useRef(opts)
  optsRef.current = opts

  useEffect(() => {
    const o = optsRef.current
    const specs: ShortcutSpec[] = [
      {
        id: 'palette',
        key: 'k',
        meta: true,
        handler: () => o.onCommandPalette(),
      },
      {
        id: 'new-session',
        key: 'n',
        meta: true,
        handler: () => o.onNewSession(),
      },
      {
        id: 'quick',
        key: 'p',
        meta: true,
        handler: () => o.onQuickSwitch?.() ?? o.onCommandPalette(),
      },
      {
        id: 'close-tab',
        key: 'w',
        meta: true,
        handler: () => o.closeActiveTab(),
      },
      {
        id: 'prev',
        key: '[',
        meta: true,
        handler: () => o.prevTab(),
      },
      {
        id: 'next',
        key: ']',
        meta: true,
        handler: () => o.nextTab(),
      },
      {
        id: 'focus-input',
        key: 'e',
        meta: true,
        handler: () => o.focusSessionInput(),
      },
      {
        id: 'help',
        key: '/',
        meta: true,
        handler: () => o.onHelpToggle(),
      },
      {
        id: 'pause',
        key: '.',
        meta: true,
        handler: () => o.pauseRunningSessions(),
      },
      {
        id: 'settings',
        key: ',',
        meta: true,
        handler: () => o.onSettings(),
      },
      {
        id: 'esc',
        key: 'Escape',
        allowInField: true,
        handler: () => o.collapseExpanded(),
      },
      {
        id: 'j',
        key: 'j',
        handler: () =>
          window.dispatchEvent(new CustomEvent('mp:message-nav', { detail: { dir: 1 as const } })),
      },
      {
        id: 'k-key',
        key: 'k',
        handler: () =>
          window.dispatchEvent(new CustomEvent('mp:message-nav', { detail: { dir: -1 as const } })),
      },
      {
        id: 'G',
        key: 'G',
        shift: true,
        handler: () => {
          const el = document.getElementById('conversation-scroll')
          el?.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
        },
      },
    ]

    for (let n = 1; n <= 9; n += 1) {
      const digit = String(n)
      specs.push({
        id: `jump-${n}`,
        key: digit,
        meta: true,
        handler: () => o.jumpTab(n - 1),
      })
    }

    return bindShortcuts(specs)
  }, [pathname])

  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      if (ev.defaultPrevented) return
      if (ev.metaKey || ev.ctrlKey || ev.altKey) return
      const t = ev.target
      if (t instanceof HTMLElement && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) return

      const el = document.getElementById('conversation-scroll')
      if (!el) return

      if (ev.key !== 'g') return
      const now = Date.now()
      if (now - lastG.current < 450) {
        el.scrollTo({ top: 0, behavior: 'smooth' })
        ev.preventDefault()
        lastG.current = 0
        return
      }
      lastG.current = now
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [pathname])
}
