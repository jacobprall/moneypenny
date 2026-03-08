# OpenClaw Integration

### V1 Integration Contract

Start with one-way ingestion from OpenClaw file logs (`logging.file` JSONL), then add a query bridge.

1. **Ingest (raw + projected):**
   - Read OpenClaw JSONL entries from `~/.openclaw/openclaw.json` -> `logging.file`.
   - Store **every** event in a raw external-events table for replay and forensic debugging.
   - Project recognized records into native tables (`sessions`, `messages`, `tool_calls`, `policy_audit`) for first-class querying.
   - Run extraction over imported conversations to promote durable facts.

2. **Idempotency and replay safety:**
   - Deduplicate by source event ID when available; otherwise use deterministic content hash.
   - Track ingest runs (cursor/offset, counts, errors) so imports are resumable and auditable.

3. **Query bridge (runtime parity):**
   - OpenClaw sessions call Moneypenny for:
     - memory retrieval before response generation,
     - policy/audit explanations ("why denied?"),
     - cross-session context ("what do we already know about this user/system?").

### Event Families to Map First

Prioritize these OpenClaw diagnostics/log families for projection:

- `model.usage` (tokens, cost, duration, provider/model/channel)
- `message.queued`, `message.processed`
- `webhook.received`, `webhook.processed`, `webhook.error`
- `session.state`, `session.stuck`
- `run.attempt`

Everything else still lands in raw event storage so no source signal is lost.

### Why This Is a Competitive Advantage

- **OpenClaw alone:** excellent execution breadth, less opinionated long-term memory/governance substrate.
- **Moneypenny alone:** strong transactional intelligence core, smaller channel/device surface.
- **Together:** broad execution + durable governed intelligence with a portable SQLite state file.

---
