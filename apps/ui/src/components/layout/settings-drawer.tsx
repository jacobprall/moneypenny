import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Drawer, DrawerClose, DrawerContent, DrawerTitle } from '@/components/ui/drawer'
import { Input } from '@/components/ui/input'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useQuery } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/rpc'
import type { SystemConfig } from '@/lib/api-types'

export function SettingsDrawer(props: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const q = useQuery({ queryKey: queryKeys.systemConfig(), queryFn: () => api.system.getConfig() })
  const [draft, setDraft] = useState<SystemConfig>({})
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (q.data) setDraft(q.data)
  }, [q.data])

  const save = async () => {
    setSaving(true)
    try {
      await api.system.setConfig(draft)
      await q.refetch()
    } finally {
      setSaving(false)
    }
  }

  return (
    <Drawer open={props.open} onOpenChange={props.onOpenChange} direction="right">
      <DrawerContent className="max-w-lg">
        <DrawerTitle>SETTINGS</DrawerTitle>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto px-4 py-3">
          <Tabs defaultValue="models">
            <TabsList>
              <TabsTrigger value="models">MODELS</TabsTrigger>
              <TabsTrigger value="policies">POLICIES</TabsTrigger>
              <TabsTrigger value="mcp">MCP</TabsTrigger>
            </TabsList>
            <TabsContent value="models" className="space-y-3">
              <label className="block text-xs text-fg-dim">strong</label>
              <Input
                value={String(draft.models?.strong ?? '')}
                onChange={(e) =>
                  setDraft((d) => ({ ...d, models: { ...d.models, strong: e.target.value } }))
                }
              />
              <label className="block text-xs text-fg-dim">fast</label>
              <Input
                value={String(draft.models?.fast ?? '')}
                onChange={(e) =>
                  setDraft((d) => ({ ...d, models: { ...d.models, fast: e.target.value } }))
                }
              />
              <label className="block text-xs text-fg-dim">local</label>
              <Input
                value={String(draft.models?.local ?? '')}
                onChange={(e) =>
                  setDraft((d) => ({ ...d, models: { ...d.models, local: e.target.value } }))
                }
              />
              <label className="block text-xs text-fg-dim">ollama base url</label>
              <Input
                value={String(draft.ollama_base_url ?? '')}
                onChange={(e) => setDraft((d) => ({ ...d, ollama_base_url: e.target.value }))}
              />
              <div className="border border-border p-3 font-mono text-xs uppercase text-fg-dim">
                sqlite-ai
              </div>
              <label className="block text-xs text-fg-dim">model dir</label>
              <Input
                value={String(draft.sqlite_ai?.model_dir ?? '')}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    sqlite_ai: { ...d.sqlite_ai, model_dir: e.target.value },
                  }))
                }
              />
              <label className="block text-xs text-fg-dim">context_size</label>
              <Input
                type="number"
                value={draft.sqlite_ai?.context_size ?? ''}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    sqlite_ai: {
                      ...d.sqlite_ai,
                      context_size: Number(e.target.value),
                    },
                  }))
                }
              />
              <label className="block text-xs text-fg-dim">n_predict</label>
              <Input
                type="number"
                value={draft.sqlite_ai?.n_predict ?? ''}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    sqlite_ai: { ...d.sqlite_ai, n_predict: Number(e.target.value) },
                  }))
                }
              />
              <label className="block text-xs text-fg-dim">gpu_layers</label>
              <Input
                type="number"
                value={draft.sqlite_ai?.gpu_layers ?? ''}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    sqlite_ai: { ...d.sqlite_ai, gpu_layers: Number(e.target.value) },
                  }))
                }
              />
            </TabsContent>
            <TabsContent value="policies" className="text-sm text-fg-dim">
              remote editing for policy rows is not wired in this build; use the filesystem per server docs.
            </TabsContent>
            <TabsContent value="mcp" className="text-sm text-fg-dim">
              mcp endpoints will map here when `system.config` includes server entries.
            </TabsContent>
          </Tabs>
          <div className="mt-auto flex gap-2 border-t border-border pt-3">
            <DrawerClose asChild>
              <Button variant="ghost">CLOSE</Button>
            </DrawerClose>
            <Button variant="primary" disabled={saving || q.isLoading} onClick={() => void save()}>
              SAVE
            </Button>
          </div>
        </div>
      </DrawerContent>
    </Drawer>
  )
}
