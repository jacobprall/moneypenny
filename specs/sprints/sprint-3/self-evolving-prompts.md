# Self-Evolving Prompts

### Problem

Agent prompts are static. A blueprint's system prompt is the same on day 1
as day 100, even though the agent has accumulated context about the user's
preferences, common patterns, and coding style. The moneypenny-rs spec
(sprint-4/self-aware-db.md) describes prompts that evolve from usage data.

### Design

```typescript
// @moneypenny/ctx

export interface PromptEvolver {
  evolve(agentName: string): Promise<PromptRefinement[]>;
  getRefinements(agentName: string): PromptRefinement[];
  setRefinementStatus(refinementId: string, status: "accepted" | "rejected"): void;
}

export interface PromptRefinement {
  id: string;
  agentName: string;
  category: RefinementCategory;
  content: string;
  confidence: number;           // 0..1
  status: "proposed" | "accepted" | "rejected";
  evidence: string;
  sourceSessionIds: string[];
  createdAt: number;
  updatedAt: number;
}

export type RefinementCategory =
  | "user_preference"
  | "common_pattern"
  | "error_prevention"
  | "tool_usage"
  | "style_guide"
  | "domain_knowledge";
```

### Schema (migration v12)

```sql
CREATE TABLE prompt_refinements (
  id TEXT PRIMARY KEY NOT NULL,
  agent_name TEXT NOT NULL,
  category TEXT NOT NULL,
  content TEXT NOT NULL,
  confidence REAL NOT NULL DEFAULT 0.5,
  status TEXT NOT NULL DEFAULT 'proposed',
  evidence TEXT,
  source_sessions TEXT,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_refinements_agent ON prompt_refinements(agent_name, status);
```

### Token budget and cap

**Problem identified in gap analysis:** Without a cap, accepted refinements
accumulate indefinitely. 20 refinements at 50 tokens each = 1000 tokens
added to every LLM call. After months, this crowds out code context.

**Solution:** Hard cap of **15 accepted refinements** per agent, with a
**750 token budget**. When a new refinement would exceed either limit,
the lowest-confidence accepted refinement is demoted to `archived`:

```typescript
const MAX_REFINEMENTS = 15;
const MAX_REFINEMENT_TOKENS = 750;

function pruneRefinements(
  refinements: PromptRefinement[],
  newRefinement: PromptRefinement,
): { accept: PromptRefinement[]; archive: PromptRefinement[] } {
  const all = [...refinements, newRefinement].sort(
    (a, b) => b.confidence - a.confidence
  );

  const accept: PromptRefinement[] = [];
  const archive: PromptRefinement[] = [];
  let tokenCount = 0;

  for (const r of all) {
    const tokens = estimateTokens(r.content);
    if (accept.length < MAX_REFINEMENTS && tokenCount + tokens <= MAX_REFINEMENT_TOKENS) {
      accept.push(r);
      tokenCount += tokens;
    } else {
      archive.push(r);
    }
  }

  return { accept, archive };
}
```

### Refinement deduplication

**Problem identified in gap analysis:** `evolve()` runs on the last 20
sessions and will propose the same refinement repeatedly if the pattern
persists.

**Solution:** Before proposing a new refinement, check existing refinements
(all statuses) for semantic overlap. Use a two-stage check:

1. **Exact substring match** — if the new content contains an existing
   refinement's content (or vice versa), treat as duplicate.
2. **LLM dedup check** — include existing refinements in the evolution
   prompt so the LLM avoids reproposing them:

```
## Existing refinements (do NOT repropose these)
{{#each existingRefinements}}
- [{{status}}] {{content}}
{{/each}}

Only propose NEW patterns not already covered above.
```

This eliminates the need for embedding-based similarity (which would add
complexity and cost). The LLM is already being called for evolution — the
dedup check is free context.

### Evolution analysis

The `evolve()` method:

1. Loads the last N sessions for the agent (default 20)
2. Loads all existing refinements (proposed, accepted, rejected)
3. Sends to LLM with the evolution prompt
4. LLM returns new refinements, avoiding duplicates of existing ones
5. New refinements are inserted as `proposed`
6. Confidence of existing accepted refinements is updated if the LLM
   confirms the pattern is still consistent

### Evolution prompt

```
Analyze these recent coding sessions for agent "{{agentName}}".

## Existing refinements (do NOT repropose these)
{{#each existingRefinements}}
- [{{status}}] (confidence: {{confidence}}) {{content}}
{{/each}}

## Sessions to analyze
{{#each sessions}}
### Session: {{label}} ({{messageCount}} messages)
{{compactedSummary || firstUserMessage}}
{{/each}}

## Task
Identify NEW recurring patterns in these categories:
1. User preferences (coding style, naming, architecture choices)
2. Common patterns (frameworks, libraries, APIs used repeatedly)
3. Error prevention (mistakes the agent made that the user corrected)
4. Tool usage patterns (which tools the user prefers for what tasks)
5. Style guides (formatting, conventions observed in accepted code)
6. Domain knowledge (business logic, API details, architecture decisions)

For each NEW pattern (not already in existing refinements), provide:
- category: one of the above
- content: a concise instruction for the agent's system prompt (max 50 words)
- confidence: 0..1 based on how consistent the pattern is across sessions
- evidence: specific session excerpts that support this

Only propose refinements with confidence >= 0.5.
Do NOT propose patterns that overlap with existing refinements.
```

### Injection into system prompt

```typescript
function buildSystemPrompt(
  blueprint: AgentConfig,
  refinements: PromptRefinement[],
): string {
  const accepted = refinements
    .filter(r => r.status === "accepted")
    .sort((a, b) => b.confidence - a.confidence);

  if (accepted.length === 0) return blueprint.systemPrompt;

  const refinementBlock = accepted
    .map(r => `- ${r.content}`)
    .join("\n");

  return `${blueprint.systemPrompt}

## Learned preferences

Based on our previous interactions, I've learned:
${refinementBlock}`;
}
```

### Auto-accept threshold

Refinements with confidence >= 0.9 and evidence from 5+ sessions are
auto-accepted. All others require explicit user acceptance via the Tune
page or `context_curate`:

```
context_curate({ action: "list_refinements", params: { agent: "default" } })
context_curate({ action: "accept_refinement", params: { id: "ref_123" } })
context_curate({ action: "reject_refinement", params: { id: "ref_123" } })
```

### User feedback via Tune page

The web UI Tune page includes a "Learned Preferences" section:

- Lists all refinements grouped by status (proposed, accepted, rejected)
- User can accept/reject proposed refinements with one click
- Shows confidence score and evidence excerpt
- Rejected refinements are excluded from future proposals
- Accepted refinements show their injection position in the system prompt

### Reactive trigger

The `session_completed` event (from §2) triggers an evolution check:

```typescript
eventBus.on("session_completed", async (event) => {
  const sessionCount = getSessionCount(db, agentName);
  const lastEvolution = getLastEvolutionRun(db, agentName);

  // Evolve every 10 sessions or every 7 days, whichever comes first
  if (sessionCount - lastEvolution.sessionCount >= 10 ||
      Date.now() / 1000 - lastEvolution.timestamp > 604800) {
    await evolver.evolve(agentName);
  }
});
```

### Acceptance criteria

- [ ] `evolve()` analyzes recent sessions and proposes new refinements
- [ ] Existing refinements are included in the prompt to prevent duplicates
- [ ] Accepted refinements appear in the system prompt as "Learned preferences"
- [ ] Max 15 accepted refinements per agent, within 750 token budget
- [ ] Lowest-confidence refinement is archived when budget is exceeded
- [ ] Auto-accept works for confidence >= 0.9 with 5+ session evidence
- [ ] Rejected refinements are excluded from future proposals
- [ ] `context_curate` exposes refinement management actions
- [ ] Tune page shows refinements grouped by status with accept/reject buttons
- [ ] Evolution triggers every 10 sessions or 7 days via reactive event

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | `prompt_refinements` schema, `PromptRefinement` types, CRUD | 1 day |
| 3.2 | `PromptEvolver.evolve()` — session analysis, dedup, LLM extraction | 3 days |
| 3.3 | Token budget pruning, auto-accept logic | 1 day |
| 3.4 | System prompt injection with accepted refinements | 0.5 days |
| 3.5 | User feedback: Tune page section + `context_curate` integration | 1.5 days |
| 3.6 | Reactive trigger: `session_completed` → evolution check | 0.5 days |
| 3.7 | Custodian integration (scheduled evolution runs) | 0.5 days |
