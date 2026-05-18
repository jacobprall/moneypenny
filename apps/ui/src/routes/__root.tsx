import type { QueryClient } from '@tanstack/react-query'
import { Outlet, createRootRouteWithContext, useNavigate } from '@tanstack/react-router'
import { useCallback, useEffect, useRef, useState } from 'react'
import { CommandPalette } from '@/components/layout/command-palette'
import { HelpOverlay } from '@/components/layout/help-overlay'
import { NewSessionDialog } from '@/components/layout/new-session-dialog'
import { SettingsDrawer } from '@/components/layout/settings-drawer'
import { TabBar } from '@/components/layout/tab-bar'
import { TopBar } from '@/components/layout/top-bar'
import { useKeyboard } from '@/hooks/use-keyboard'
import { useTabs } from '@/hooks/use-tabs'
import { api } from '@/lib/rpc'

export interface RouterAppContext {
  queryClient: QueryClient
}

export const rootRoute = createRootRouteWithContext<RouterAppContext>()({
  component: RootLayout,
})

function RootLayout() {
  const navigate = useNavigate()
  const { tabs, closeTab, setActiveTab, openTab, reorderTabs } = useTabs()

  const [paletteOpen, setPaletteOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [newSessionOpen, setNewSessionOpen] = useState(false)
  const [helpOpen, setHelpOpen] = useState(false)

  const didInit = useRef(false)
  useEffect(() => {
    if (didInit.current || !tabs.length) return
    didInit.current = true
    const active = tabs.find((t) => t.active) ?? tabs[0]
    if (!active) return
    if (active.kind === 'session' && active.session_id) {
      navigate({ to: '/s/$sessionId', params: { sessionId: active.session_id } })
      return
    }
    if (active.kind === 'ideas') {
      navigate({ to: '/ideas' })
      return
    }
    if (active.kind === 'search') {
      navigate({ to: '/search' })
    }
  }, [tabs, navigate])

  const activeTab = tabs.find((t) => t.active)

  const navigateForTab = useCallback(
    async (t: (typeof tabs)[number]) => {
      await setActiveTab(t.id)
      if (t.kind === 'session' && t.session_id) {
        navigate({ to: '/s/$sessionId', params: { sessionId: t.session_id } })
      } else if (t.kind === 'ideas') navigate({ to: '/ideas' })
      else if (t.kind === 'search') navigate({ to: '/search' })
      else navigate({ to: '/' })
    },
    [navigate, setActiveTab],
  )

  const closeActiveTab = useCallback(async () => {
    if (!activeTab) return
    await closeTab(activeTab.id)
  }, [activeTab, closeTab])

  const prevTab = useCallback(async () => {
    if (!tabs.length) return
    const idx = tabs.findIndex((t) => t.id === activeTab?.id)
    const next = tabs[(idx - 1 + tabs.length) % tabs.length]
    if (next) await navigateForTab(next)
  }, [tabs, activeTab, navigateForTab])

  const nextTab = useCallback(async () => {
    if (!tabs.length) return
    const idx = tabs.findIndex((t) => t.id === activeTab?.id)
    const next = tabs[(idx + 1) % tabs.length]
    if (next) await navigateForTab(next)
  }, [tabs, activeTab, navigateForTab])

  const jumpTab = useCallback(
    async (i: number) => {
      const t = tabs[i]
      if (t) await navigateForTab(t)
    },
    [tabs, navigateForTab],
  )

  const pauseRunningSessions = useCallback(async () => {
    const running = tabs.filter((t) => t.kind === 'session' && t.session_id)
    await Promise.all(
      running.map((t) => (t.session_id ? api.sessions.pause(t.session_id) : Promise.resolve())),
    )
  }, [tabs])

  useKeyboard({
    onCommandPalette: () => setPaletteOpen(true),
    onNewSession: () => setNewSessionOpen(true),
    onSettings: () => setSettingsOpen(true),
    onHelpToggle: () => setHelpOpen((h) => !h),
    closeActiveTab,
    prevTab,
    nextTab,
    jumpTab,
    focusSessionInput: () => window.dispatchEvent(new CustomEvent('mp:focus-session-input')),
    collapseExpanded: () => window.dispatchEvent(new CustomEvent('mp:collapse-toolcalls')),
    pauseRunningSessions,
  })

  return (
    <div className="boot-fade boot-scanline flex min-h-screen flex-col bg-canvas text-fg">
      <TopBar onOpenPalette={() => setPaletteOpen(true)} onOpenSettings={() => setSettingsOpen(true)} />
      <TabBar
        tabs={tabs}
        onActivate={navigateForTab}
        onClose={(id) => void closeTab(id)}
        onReorder={(ordered) => void reorderTabs(ordered)}
        onNewSession={() => setNewSessionOpen(true)}
      />
      <main className="min-h-0 flex-1 overflow-hidden">
        <Outlet />
      </main>
      <CommandPalette
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
        tabs={tabs}
        onSelectTab={(t) => void navigateForTab(t)}
        onNewSession={() => {
          setPaletteOpen(false)
          setNewSessionOpen(true)
        }}
      />
      <SettingsDrawer open={settingsOpen} onOpenChange={setSettingsOpen} />
      <NewSessionDialog
        open={newSessionOpen}
        onOpenChange={setNewSessionOpen}
        onLaunched={async (sessionId) => {
          await openTab({ kind: 'session', session_id: sessionId, label: 'session' })
          navigate({ to: '/s/$sessionId', params: { sessionId } })
        }}
      />
      <HelpOverlay open={helpOpen} onOpenChange={setHelpOpen} />
    </div>
  )
}
