import ReactMarkdown from 'react-markdown'
import { useNavigate } from '@tanstack/react-router'
import rehypeSanitize from 'rehype-sanitize'
import remarkGfm from 'remark-gfm'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Drawer, DrawerClose, DrawerContent, DrawerTitle } from '@/components/ui/drawer'
import { NewSessionDialog } from '@/components/layout/new-session-dialog'
import type { Idea } from '@/lib/api-types'
import { useTabs } from '@/hooks/use-tabs'

export function IdeaDetailDrawer(props: {
  idea: Idea | null
  onOpenChange: (open: boolean) => void
}) {
  const [launchOpen, setLaunchOpen] = useState(false)
  const navigate = useNavigate()
  const { openTab } = useTabs()
  const open = props.idea !== null

  if (!props.idea) return null

  const fm = props.idea.frontmatter

  return (
    <>
      <Drawer open={open} onOpenChange={props.onOpenChange} direction="right">
        <DrawerContent className="max-w-xl">
          <DrawerTitle className="truncate">{fm.title ?? props.idea.filename}</DrawerTitle>
          <div className="space-y-4 overflow-auto px-4 py-3">
            <details className="border border-border bg-canvas">
              <summary className="cursor-pointer px-3 py-2 font-mono text-xs uppercase text-fg-dim">
                frontmatter
              </summary>
              <pre className="max-h-48 overflow-auto border-t border-border p-3 font-mono text-[11px] text-fg">
                {JSON.stringify(fm, null, 2)}
              </pre>
            </details>
            <div className="border border-border bg-panel p-4 font-sans text-sm text-fg">
              <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeSanitize]}>{props.idea.body}</ReactMarkdown>
            </div>
            <div className="flex gap-2">
              <Button type="button" variant="primary" size="sm" onClick={() => setLaunchOpen(true)}>
                launch session from idea
              </Button>
              <DrawerClose asChild>
                <Button variant="ghost" size="sm">
                  close
                </Button>
              </DrawerClose>
            </div>
          </div>
        </DrawerContent>
      </Drawer>
      <NewSessionDialog
        open={launchOpen}
        onOpenChange={setLaunchOpen}
        linkedIdeaFilename={props.idea.filename}
        onLaunched={async (sessionId) => {
          await openTab({ kind: 'session', session_id: sessionId, label: props.idea?.filename ?? 'session' })
          setLaunchOpen(false)
          props.onOpenChange(false)
          navigate({ to: '/s/$sessionId', params: { sessionId } })
        }}
      />
    </>
  )
}
