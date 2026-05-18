import { useCallback, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { Drawer, DrawerClose, DrawerContent, DrawerTitle } from '@/components/ui/drawer'
import { Input } from '@/components/ui/input'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import type { Session } from '@/lib/api-types'
import { formatUsd } from '@/lib/format'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'

export function SessionFooter(props: { session: Session; costUsd?: number }) {
  const qc = useQueryClient()
  const [bpOpen, setBpOpen] = useState(false)
  const [cwdOpen, setCwdOpen] = useState(false)
  const [cwdDraft, setCwdDraft] = useState(props.session.cwd)

  const bp = useQuery({
    queryKey: [...queryKeys.blueprints(), props.session.blueprint],
    queryFn: () => api.blueprints.get(props.session.blueprint),
    enabled: bpOpen,
  })

  const submitCwd = useCallback(async () => {
    const trimmed = cwdDraft.trim()
    if (!trimmed || trimmed === props.session.cwd) {
      setCwdOpen(false)
      return
    }
    await api.sessions.patchConfig(props.session.id, {
      config: { cwd: trimmed },
      config_version: props.session.config_version ?? 0,
    })
    void qc.invalidateQueries({ queryKey: queryKeys.session(props.session.id) })
    setCwdOpen(false)
  }, [cwdDraft, props.session.id, props.session.cwd, props.session.config_version, qc])

  return (
    <>
      <footer className="flex flex-wrap items-center gap-3 border-t border-border px-4 py-2 font-mono text-xs text-fg-dim">
        <Button
          type="button"
          variant="link"
          className="text-xs uppercase"
          onClick={() => {
            setCwdDraft(props.session.cwd)
            setCwdOpen(true)
          }}
        >
          {props.session.cwd}
        </Button>
        <span aria-hidden>·</span>
        <Button type="button" variant="link" className="text-xs uppercase" onClick={() => setBpOpen(true)}>
          {props.session.blueprint}
        </Button>
        <span aria-hidden>·</span>
        <span className="tabular-nums text-fg">{formatUsd(props.costUsd)}</span>
      </footer>

      <Dialog open={cwdOpen} onOpenChange={setCwdOpen}>
        <DialogContent className="max-w-md gap-4">
          <DialogTitle>CHANGE CWD</DialogTitle>
          <Input
            value={cwdDraft}
            onChange={(e) => setCwdDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void submitCwd()
            }}
            className="font-mono text-sm"
            autoFocus
          />
          <div className="flex justify-end gap-2 border-t border-border pt-3">
            <Button variant="ghost" onClick={() => setCwdOpen(false)}>
              cancel
            </Button>
            <Button variant="primary" onClick={() => void submitCwd()}>
              set
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      <Drawer open={bpOpen} onOpenChange={setBpOpen} direction="right">
        <DrawerContent className="max-w-lg">
          <DrawerTitle>BLUEPRINT</DrawerTitle>
          <pre className="max-h-[70vh] overflow-auto whitespace-pre-wrap border-t border-border p-4 font-mono text-xs text-fg">
            {bp.data?.raw ?? 'loading…'}
          </pre>
          <DrawerClose asChild>
            <Button variant="ghost" className="m-4">
              close
            </Button>
          </DrawerClose>
        </DrawerContent>
      </Drawer>
    </>
  )
}
