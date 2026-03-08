# Moneypenny — Feature Demo Guide

A step-by-step walkthrough for a human presenter. Not a runnable script — read
each section, type the commands, and talk through what's happening.

**Time estimate:** 30–45 minutes depending on pace.

**Prerequisites:**
- Rust toolchain installed (`rustup`)
- Repository cloned with submodules (`git submodule update --init --recursive`)
- No prior `moneypenny.toml` or `mp-data/` in the repo root (or willingness to wipe them)

---

## 0. Build

```
cargo build
```

Confirm the binary exists:

```
ls -la target/debug/mp
```

If you want local embedding (vector search), download the model:

```
mkdir -p mp-data/models
curl -L -o mp-data/models/nomic-embed-text-v1.5.gguf \
  https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf
```

> **What to say:** "Moneypenny is a single Rust binary. Everything runs locally —
> no containers, no cloud services, no daemon to install."

---

## 1. Initialize a Project

Remove any prior state, then initialize:

```
rm -rf mp-data moneypenny.toml
./target/debug/mp init
```

Look at what was created:

```
cat moneypenny.toml
ls mp-data/
```

Verify the system is healthy:

```
./target/debug/mp health
```

> **What to say:** "`mp init` creates one config file and one SQLite database per
> agent. That database file IS the agent — memory, policy, audit, skills, knowledge,
> everything in one portable file."

---

## 2. Agent Management

List the agents that were bootstrapped:

```
./target/debug/mp agent list
```

Check the default agent's status (empty so far):

```
./target/debug/mp agent status
```

Create a second agent:

```
./target/debug/mp agent create research
./target/debug/mp agent list
```

> **What to say:** "Agents are registered in a metadata DB and each gets its own
> SQLite file. Creating an agent goes through the canonical operation pipeline —
> same policy checks, hooks, and audit as any other mutation."

---

## 3. Knowledge Ingestion — Local File

Ingest the project handbook into the knowledge store:

```
./target/debug/mp ingest scripts/demo-data/project-handbook.md
```

List what was ingested:

```
./target/debug/mp knowledge list
```

Search across the knowledge base:

```
./target/debug/mp knowledge search "deployment pipeline"
./target/debug/mp knowledge search "security"
```

> **What to say:** "Documents are chunked and stored in SQLite with FTS5 full-text
> indexes. No external vector DB needed — search works immediately."

---

## 4. Knowledge Ingestion — URL

If you have a public URL to demo with:

```
./target/debug/mp ingest --url "https://example.com/some-doc"
./target/debug/mp knowledge list
```

> **What to say:** "Same pipeline, same storage. URL content is fetched, chunked,
> and indexed identically to local files."

---

## 5. External Event Import (OpenClaw/JSONL)

Import a conversation history from an external agent runtime:

```
./target/debug/mp ingest --openclaw-file scripts/demo-data/openclaw-history.jsonl --source openclaw
```

Check the ingest status:

```
./target/debug/mp ingest --status --source openclaw
```

> **What to say:** "Moneypenny can import event histories from any JSONL-based
> agent runtime — sessions, messages, model usage, webhooks, run attempts. Events
> are deduplicated by content hash so re-imports are safe. Facts are automatically
> extracted from imported conversations."

Show the replay safety — run the same import again:

```
./target/debug/mp ingest --openclaw-file scripts/demo-data/openclaw-history.jsonl --source openclaw
```

> **What to say:** "Second import: zero new events. Content-hash dedup makes ingest
> idempotent."

---

## 6. Structured Memory — Facts

Add facts via the sidecar canonical-ops interface (no LLM needed):

```
echo '{"op":"memory.fact.add","args":{"content":"The deployment pipeline uses ArgoCD with lint, unit tests, integration tests, canary (5% traffic for 30 minutes), then full rollout. Deploys happen Tue/Thu. Rollbacks auto-trigger if error rate exceeds 0.1%.","summary":"ArgoCD pipeline: lint-test-canary(5%/30min)-rollout Tue/Thu, auto-rollback at 0.1%","pointer":"DEPLOY: argo-pipeline","confidence":0.95,"keywords":"deployment argocd canary rollback"}}' \
  | ./target/debug/mp sidecar
```

```
echo '{"op":"memory.fact.add","args":{"content":"Fleet of 200 autonomous warehouse robots across Austin, Chicago, and Newark. Four core services: Navigator (LiDAR path planning), Picker (6-DOF arm), Orchestrator (fleet coordination), Vision (YOLOv8 on edge TPUs). All communicate over gRPC.","summary":"200 robots, 3 sites, 4 services (Navigator/Picker/Orchestrator/Vision) over gRPC","pointer":"FLEET: 200-robots-3-sites","confidence":0.92,"keywords":"robots fleet architecture navigator picker orchestrator vision"}}' \
  | ./target/debug/mp sidecar
```

```
echo '{"op":"memory.fact.add","args":{"content":"Performance metrics: fulfillment rate 99.7% (target 99.9%), mean pick time 4.2s (target 3.5s), robot uptime 98.1% (target 99.5%), MTTR 12min (target 10min). Main bottleneck is arm trajectory planning.","summary":"Metrics below target: pick 4.2s/3.5s, uptime 98.1%/99.5%, MTTR 12/10min","pointer":"PERF: metrics-below-target","confidence":0.88,"keywords":"metrics performance pick time uptime bottleneck"}}' \
  | ./target/debug/mp sidecar
```

```
echo '{"op":"memory.fact.add","args":{"content":"Top priorities: 1) Reduce pick time 4.2s to 3.5s via trajectory optimization, 2) Roll out firmware v3.2 to all sites, 3) Complete SOC 2 Type II by end of Q2, 4) Hire 3 senior perception engineers, 5) Migrate CockroachDB to TiKV for 40% cost savings.","summary":"Top 5: pick time opt, fw v3.2 rollout, SOC2 Q2, hire 3 eng, CRDB-to-TiKV","pointer":"PRIORITIES: top-5-q2","confidence":0.90,"keywords":"priorities firmware soc2 hiring tikv migration"}}' \
  | ./target/debug/mp sidecar
```

List all facts:

```
./target/debug/mp facts list
```

> **What to say:** "Every fact has three compression levels: the full content,
> a one-line summary, and a 2–5 word pointer. In context assembly, facts
> compact progressively as the token budget fills — full text first, then
> summaries, then just pointers. The agent can always expand a pointer to get
> the full content back."

---

## 7. Fact Inspection and Expansion

Pick a fact ID from the list output above, then:

```
./target/debug/mp facts inspect <FACT_ID>
```

> **What to say:** "Inspect shows the full content, summary, pointer, confidence
> score, compaction level, version history, and audit trail — all stored in the
> same SQLite file."

Expand a compacted pointer back to its full text:

```
./target/debug/mp facts expand <FACT_ID>
```

---

## 8. Hybrid Search

Search across facts (FTS + vector when embeddings are available):

```
./target/debug/mp facts search "ArgoCD deployment"
./target/debug/mp facts search "bottleneck"
./target/debug/mp facts search "security"
```

Search across knowledge chunks:

```
./target/debug/mp knowledge search "trajectory optimization"
```

> **What to say:** "Search is hybrid: FTS5 full-text matching plus vector KNN
> (when embeddings exist), fused via Reciprocal Rank Fusion with MMR re-ranking
> for diversity. It searches across all stores — facts, messages, tool calls,
> knowledge chunks, and policy audit — in a single query."

---

## 9. Policy Engine

List the default policies:

```
./target/debug/mp policy list
```

Add a deny rule to block destructive SQL:

```
./target/debug/mp policy add \
  --name "no-destructive-sql" \
  --effect deny \
  --action "execute" \
  --resource "sql:*DROP*" \
  --message "Destructive SQL is blocked by policy"
```

Add an audit rule to log all memory searches:

```
./target/debug/mp policy add \
  --name "audit-memory-searches" \
  --effect audit \
  --action "search" \
  --resource "memory"
```

Verify:

```
./target/debug/mp policy list
```

Test policy evaluation (dry run):

```
./target/debug/mp policy test "DROP TABLE facts"
./target/debug/mp policy test "SELECT * FROM facts"
```

Check for recent violations:

```
./target/debug/mp policy violations
```

> **What to say:** "Policies use glob patterns for actor/action/resource matching.
> Three effects: allow, deny, audit. Every policy decision is logged with the full
> request context. Denials are explainable — the agent can ask why something was
> blocked."

---

## 10. Policy Explanation via Sidecar

Ask the system to explain a policy decision:

```
echo '{"op":"policy.evaluate","args":{"actor":"main","action":"execute","resource":"sql:DROP TABLE users"}}' \
  | ./target/debug/mp sidecar
```

```
echo '{"op":"policy.explain","args":{"actor":"main","action":"call","resource":"web_search"}}' \
  | ./target/debug/mp sidecar
```

> **What to say:** "Any surface — CLI, HTTP, MCP, sidecar — gets the same policy
> evaluation. The canonical operation layer is the single enforcement point."

---

## 11. Skills

Register a reusable skill:

```
echo '{"op":"skill.add","args":{"name":"incident-triage","description":"Triage production incidents using the escalation runbook","content":"When triaging: 1) Check dashboards for scope. 2) Escalation: on-call → team lead → VP Eng → CTO. 3) Single-robot: SRE investigates. 4) Fleet-wide: escalate to platform lead immediately. 5) Document in incident channel. 6) Post-incident review within 48h."}}' \
  | ./target/debug/mp sidecar
```

List skills:

```
./target/debug/mp skill list
```

> **What to say:** "Skills are reusable procedures stored in the agent's DB.
> They're discoverable via RAG — when the agent gets a question about incident
> handling, the skill shows up in context assembly automatically."

---

## 12. Scheduled Jobs

Create a scheduled job:

```
./target/debug/mp job create \
  --name "daily-metrics-check" \
  --schedule "0 9 * * *" \
  --job-type prompt \
  --payload '{"prompt":"Check the latest performance metrics and summarize any regressions."}'
```

List jobs:

```
./target/debug/mp job list
```

Trigger the job manually:

```
./target/debug/mp job run <JOB_ID>
```

View run history:

```
./target/debug/mp job history
```

Pause the job:

```
./target/debug/mp job pause <JOB_ID>
./target/debug/mp job list
```

> **What to say:** "Jobs support cron schedules with four payload types: prompt,
> tool, js (QuickJS sandbox), and pipeline. Overlap policies control what happens
> when a job fires while the previous run is still going. Job creation by the
> agent itself follows the same plan → confirm → apply flow."

---

## 13. Sessions

List conversation sessions:

```
./target/debug/mp session list
```

> **What to say:** "Every conversation gets a session ID. Sessions track messages,
> tool calls, rolling summaries, and are scoped to an agent. You can resume any
> session by passing `--session-id`."

---

## 14. Interactive Chat (LLM Required)

> Skip this section if you don't have an LLM configured (local model or API key).

Start an interactive chat session:

```
./target/debug/mp chat
```

Try these in the chat:

```
> What do you know about our deployment pipeline?
> What are our top priorities?
> /facts
> /scratch
> /session
> /quit
```

> **What to say:** "The agent assembles context from facts, knowledge, session
> history, skills, and scratch — all from SQLite. Tool calls (memory search,
> fact storage, web search, etc.) go through the policy engine. Fact extraction
> runs after each turn — the agent learns automatically from the conversation."

---

## 15. One-Shot Send (LLM Required)

> Skip this section if you don't have an LLM configured.

```
./target/debug/mp send main "What are the current performance bottlenecks for the robot fleet?"
```

> **What to say:** "Same agent turn pipeline as chat, but non-interactive.
> Useful for scripting, CI pipelines, or Slack/Discord bot integrations."

---

## 16. Multi-Agent CRDT Sync

Check sync status:

```
./target/debug/mp sync status
```

Push the main agent's facts to the research agent:

```
./target/debug/mp sync push --to research
```

Verify that research now has the facts:

```
./target/debug/mp db query "SELECT count(*) as fact_count FROM facts WHERE status='active'"
```

> If you have `sqlite3` installed, you can also query the research DB directly:

```
sqlite3 mp-data/research.db "SELECT pointer FROM facts WHERE status='active';"
```

Pull changes back (no-op since research hasn't added anything):

```
./target/debug/mp sync pull --from research
```

> **What to say:** "Sync uses CRDTs at the SQLite level — merge-safe, no
> conflicts. Push/pull between local agent files, or bidirectional sync with
> a cloud backend. The sync surface is data-plane only — it moves rows, not
> operations."

---

## 17. Audit Trail

View the full audit trail:

```
./target/debug/mp audit
```

Search for specific audit entries:

```
./target/debug/mp audit search "policy"
```

Export the audit trail:

```
./target/debug/mp audit export --format json
```

> **What to say:** "Every operation, policy decision, tool call, and data
> mutation is recorded in the audit trail. It's queryable, exportable, and
> stored in the same SQLite file — no external logging service needed."

---

## 18. Canonical Operations via Sidecar (MCP/Integration Surface)

The sidecar accepts JSONL over stdin and returns results on stdout. This is
the integration surface for MCP adapters, HTTP APIs, and external tooling.

```
echo '{"op":"memory.search","args":{"query":"deployment","limit":5}}' \
  | ./target/debug/mp sidecar
```

```
echo '{"op":"session.list","args":{"agent_id":"main","limit":5}}' \
  | ./target/debug/mp sidecar
```

```
echo '{"op":"audit.query","args":{"limit":5}}' \
  | ./target/debug/mp sidecar
```

> **What to say:** "Every mutating or query operation has a canonical name and
> goes through the same pipeline: parse → context → policy → pre-hooks →
> handler → post-hooks → redaction → audit. CLI, HTTP, MCP, sidecar — all
> are thin adapters over the same operation layer."

---

## 19. Portability — One File IS the Agent

Show the agent database file:

```
ls -lh mp-data/main.db
```

Show what's inside with raw SQL:

```
./target/debug/mp db schema
```

```
./target/debug/mp db query "SELECT id, pointer, confidence FROM facts WHERE status='active' ORDER BY confidence DESC"
./target/debug/mp db query "SELECT id, title, chunk_count FROM documents"
./target/debug/mp db query "SELECT name, effect, actor_pattern, resource_pattern FROM policies ORDER BY priority DESC LIMIT 5"
./target/debug/mp db query "SELECT source, count(*) as events FROM external_events GROUP BY source"
```

> **What to say:** "That single `.db` file contains facts, knowledge, skills,
> policies, sessions, messages, tool calls, audit trail, and external events.
> Copy it to another machine, open it with any SQLite client, back it up with
> `cp`. The database is the runtime."

---

## 20. Gateway Mode (Multi-Agent, HTTP/Slack/Discord)

> This section demonstrates the full production topology. Requires either an
> LLM API key or a local model. Configure channels in `moneypenny.toml` first.

Start the gateway:

```
./target/debug/mp start
```

> This spawns one worker process per agent, starts the scheduler loop, and
> (if configured) binds the HTTP server with Slack/Discord webhook endpoints.

In a separate terminal, send a message via HTTP:

```
curl -X POST http://127.0.0.1:4821/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"agent":"main","message":"What are our top priorities?"}'
```

Or route a canonical operation over HTTP:

```
curl -X POST http://127.0.0.1:4821/v1/ops \
  -H "Content-Type: application/json" \
  -d '{"op":"memory.search","args":{"query":"deployment","limit":3}}'
```

Stop the gateway:

```
./target/debug/mp stop
```

> **What to say:** "Gateway mode runs agents as supervised worker processes.
> Channel adapters (HTTP, Slack, Discord, Telegram) route messages through
> the same WorkerBus. The HTTP parity layer exposes every canonical operation
> at `/v1/ops`. Agent-to-agent delegation is supported with depth limiting."

---

## 21. Fact Lifecycle — Delete and Promote

Delete a fact (requires confirmation):

```
./target/debug/mp facts delete <FACT_ID> --confirm
./target/debug/mp facts list
```

Promote a fact to shared scope (visible to other agents):

```
./target/debug/mp facts promote <FACT_ID> --scope shared
```

Reset compaction on a fact (restores full text in context):

```
./target/debug/mp facts reset-compaction <FACT_ID>
```

Or reset all facts at once:

```
./target/debug/mp facts reset-compaction --all --confirm
```

> **What to say:** "Facts have a full lifecycle: add, update (versioned),
> compact (progressive), expand, promote (scope), delete (soft). Every
> mutation is audited."

---

## 22. JS Tool Extension (Agent-Created Tools)

Register a custom JS tool via the sidecar:

```
echo '{"op":"js.tool.add","args":{"name":"celsius_to_fahrenheit","description":"Convert Celsius to Fahrenheit","source":"function run(args) { return String(args.celsius * 9/5 + 32) + \" °F\"; }","parameters_schema":"{\"celsius\":\"number\"}"}}' \
  | ./target/debug/mp sidecar
```

List JS tools:

```
echo '{"op":"js.tool.list","args":{}}' \
  | ./target/debug/mp sidecar
```

> **What to say:** "Agents can create their own tools at runtime using JavaScript
> executed in a QuickJS sandbox. Tool creation goes through the same canonical
> operation pipeline — policy-gated, audited, and available to the LLM on the
> next turn."

Delete the tool when done:

```
echo '{"op":"js.tool.delete","args":{"name":"celsius_to_fahrenheit"}}' \
  | ./target/debug/mp sidecar
```

---

## 23. Policy File Loading

If you have a JSON policy file:

```
./target/debug/mp policy load path/to/policies.json
```

> **What to say:** "Policies can be loaded from files for repeatable governance
> configuration. Same pipeline — each rule goes through canonical `policy.add`."

---

## Recap

| # | Feature | Key Command |
|---|---------|-------------|
| 1 | Project init | `mp init` |
| 2 | Agent management | `mp agent create/list/status/delete` |
| 3 | Knowledge ingestion (file) | `mp ingest <path>` |
| 4 | Knowledge ingestion (URL) | `mp ingest --url <url>` |
| 5 | External event import | `mp ingest --openclaw-file <file>` |
| 6 | Structured facts | `mp facts list/inspect/expand` |
| 7 | Hybrid search | `mp facts search / mp knowledge search` |
| 8 | Policy engine | `mp policy add/list/test/violations` |
| 9 | Skills | `mp skill add/list` |
| 10 | Scheduled jobs | `mp job create/list/run/pause/history` |
| 11 | Sessions | `mp session list` |
| 12 | Interactive chat | `mp chat` |
| 13 | One-shot send | `mp send <agent> <message>` |
| 14 | CRDT sync | `mp sync status/push/pull/now` |
| 15 | Audit trail | `mp audit / mp audit search / mp audit export` |
| 16 | Canonical sidecar | `mp sidecar` (JSONL over stdio) |
| 17 | Gateway mode | `mp start / mp stop` |
| 18 | SQL introspection | `mp db query / mp db schema` |
| 19 | JS tool extension | `js.tool.add/list/delete` via ops |
| 20 | Portability | `cp mp-data/main.db backup.db` |

**Core message:** Everything is local, everything is SQLite, everything is
auditable. One binary, one file per agent, no cloud dependencies.
