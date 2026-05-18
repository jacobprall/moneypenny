import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '@/components/ui/dialog'
import { Textarea } from '@/components/ui/textarea'
import { api } from '@/lib/rpc'

export function IdeaEditor(props: { open: boolean; onOpenChange: (v: boolean) => void }) {
  const [body, setBody] = useState('')
  const [filename, setFilename] = useState('')

  useEffect(() => {
    if (!props.open) return
    const today = new Date().toISOString().slice(0, 10)
    setFilename(`idea-${today}.md`)
    setBody(
      `---\ntitle: \nstatus: raw\ncreated_at: ${today}\nupdated_at: ${today}\n---\n\n# title\n\n`,
    )
  }, [props.open])

  const save = async () => {
    await api.ideas.create({ filename, body })
    props.onOpenChange(false)
  }

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogTitle>NEW IDEA</DialogTitle>
        <Textarea className="min-h-[320px] font-mono text-xs" value={body} onChange={(e) => setBody(e.target.value)} />
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => props.onOpenChange(false)}>
            cancel
          </Button>
          <Button variant="primary" onClick={() => void save()}>
            save
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
