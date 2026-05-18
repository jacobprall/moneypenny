import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'

export function NewSessionDialog(props: {
  open: boolean
  onOpenChange: (v: boolean) => void
  onLaunched: (sessionId: string) => void
  linkedIdeaFilename?: string
}) {
  const [cwd, setCwd] = useState('.')
  const [blueprint, setBlueprint] = useState('default')
  const [label, setLabel] = useState('')
  const [task, setTask] = useState('')
  const [pickerPath, setPickerPath] = useState('.')

  const bpQ = useQuery({
    queryKey: queryKeys.blueprints(),
    queryFn: () => api.blueprints.list(),
    enabled: props.open,
  })
  const filesQ = useQuery({
    queryKey: ['files', 'list', pickerPath],
    queryFn: () => api.files.list(pickerPath),
    enabled: props.open,
  })

  const submit = async () => {
    const res = await api.agents.launch({
      cwd,
      blueprint,
      label: label || undefined,
      task: task || undefined,
      idea_id: props.linkedIdeaFilename,
    })
    props.onLaunched(res.session.id)
    props.onOpenChange(false)
  }

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="gap-4">
        <DialogTitle>NEW SESSION</DialogTitle>
        <DialogDescription className="font-mono text-xs text-fg-dim">
          pick blueprint, cwd, label, and optional task. submits to agents.launch.
        </DialogDescription>
        <div className="space-y-3">
          <label className="text-xs text-fg-dim">blueprint</label>
          <Select value={blueprint} onValueChange={setBlueprint}>
            <SelectTrigger>
              <SelectValue placeholder="blueprint" />
            </SelectTrigger>
            <SelectContent>
              {(bpQ.data?.items ?? []).map((b) => (
                <SelectItem key={b.name} value={b.name}>
                  {b.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <label className="text-xs text-fg-dim">cwd</label>
          <Input value={cwd} onChange={(e) => setCwd(e.target.value)} />
          <div className="border border-border p-2 font-mono text-xs text-fg-dim">
            folder picker (files.list at {pickerPath})
            <div className="mt-2 max-h-32 overflow-auto border border-border bg-canvas">
              {(filesQ.data?.items ?? []).map((f) => (
                <button
                  key={f.path}
                  type="button"
                  className="block w-full px-2 py-1 text-left text-fg hover:bg-panel-active"
                  onClick={() => {
                    if (f.kind === 'dir') {
                      setPickerPath(f.path)
                      setCwd(f.path)
                    }
                  }}
                >
                  {f.name}
                </button>
              ))}
            </div>
          </div>
          <label className="text-xs text-fg-dim">label</label>
          <Input value={label} onChange={(e) => setLabel(e.target.value)} />
          <label className="text-xs text-fg-dim">initial task</label>
          <Textarea value={task} onChange={(e) => setTask(e.target.value)} />
        </div>
        <div className="flex justify-end gap-2 border-t border-border pt-3">
          <Button variant="ghost" onClick={() => props.onOpenChange(false)}>
            cancel
          </Button>
          <Button variant="primary" onClick={() => void submit()}>
            launch
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
