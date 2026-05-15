# `context_curate` Tool

### Design

A governed tool that provides semantic operations over the intelligence
file. The agent can inspect its own state, search across knowledge
surfaces, and perform maintenance operations.

```typescript
const contextCurateTool = defineTool({
  name: "context_curate",
  description: "Query and manage your own intelligence file.",
  parameters: z.object({
    action: z.enum([
      "search_memory",
      "forget_memory",
      "review_costs",
      "list_skills",
      "update_skill",
      "list_sessions",
      "summarize_session",
      "index_status",
      "inspect_policies",
      "prune_stale_chunks",
    ]),
    params: z.record(z.unknown()).optional(),
  }),
});
```

### Governance

Destructive actions (`forget_memory`, `update_skill`, `summarize_session`,
`prune_stale_chunks`) are gated by policy. Default policy scaffolded by
`mp init`:

```yaml
- name: curation-guard
  effect: confirm
  tool_pattern: "context_curate"
  args_pattern: '{"action":"forget_memory|update_skill"}'
  message: "This action modifies your knowledge base. Proceed?"
  priority: 100
```

Note: `prune_stale_chunks` and `summarize_session` use `effect: allow`
by default since they're non-destructive (pruning removes chunks for
deleted files; summarizing doesn't delete messages).

### Acceptance criteria

- [ ] Read-only actions (`search_memory`, `review_costs`, `list_*`, `index_status`, `inspect_policies`) work without governance gates
- [ ] Destructive actions trigger policy evaluation
- [ ] `search_memory` returns results from messages, skills, and knowledge
- [ ] `review_costs` returns per-agent and aggregate cost data
- [ ] `prune_stale_chunks` removes chunks for files no longer on disk

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 7.1 | Tool definition + read-only actions | 1.5 days |
| 7.2 | Destructive actions | 1.5 days |
| 7.3 | Default curation policy | 0.5 days |
