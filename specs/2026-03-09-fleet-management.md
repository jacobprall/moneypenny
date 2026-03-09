# Fleet Management & Enterprise Deployment

**Date:** 2026-03-09
**Status:** Plan
**Scope:** Tooling for platform engineering teams deploying Moneypenny across many coding agents

---

## Problem

A platform engineering team wants to deploy Moneypenny across 200+ developers, each running a coding agent (Cursor, Claude Code, Cortex, etc.). Today, Moneypenny is a single-user tool: one binary, one config, one SQLite file. There is no fleet orchestration layer — no way to provision instances from templates, push org-wide policies, aggregate audit, or manage knowledge at scale.

Enterprise teams care about security, isolation, reproducibility, cost governance, and observability. The single-file SQLite architecture is a massive advantage here (each agent's brain is one copyable, shippable, inspectable file), but the orchestration layer on top doesn't exist yet.

---

## Design principles

1. **The SQLite-per-agent model is the isolation boundary.** Don't fight it. Each agent gets its own DB. No multi-tenant shared database.
2. **Push config, don't pull state.** The central system pushes policies and knowledge down. It pulls audit and telemetry up. It never needs direct access to agent DBs.
3. **Org policies are immutable at the instance level.** Agents can add local policies, but cannot override or disable org-enforced rules.
4. **The sidecar stays local.** The `mp` binary runs on the developer's machine for latency and offline capability. The expensive/sensitive part (LLM calls) routes through a central proxy.

---

## 1. Fleet provisioning and lifecycle

### Problem
A new developer joins. They need a Moneypenny instance preconfigured with org policies, team knowledge, the right LLM provider credentials, and the right trust level. When they leave, their instance needs to be deprovisioned and their private data purged, but team-shared facts should persist.

### Proposed tooling

```
mp fleet init --template senior-engineer    # create instance from template
mp fleet deprovision --agent jane           # purge private data, preserve shared facts
```

- **Instance templates** — version-controlled definitions that bake in org config, baseline policies, seed knowledge, and persona. Stored in a central Git repo or registry.
- **Instance registry** — central catalog of every Moneypenny instance: owner, template version, binary version, last heartbeat, DB size.
- **Lifecycle hooks** — provision on HR onboard event, deprovision on offboard. Shared facts survive; private facts are scrubbed.

### Data model (registry)

| Field | Type | Description |
|-------|------|-------------|
| instance_id | UUID | Unique instance identifier |
| owner | string | Developer identity (email, SSO ID) |
| template | string | Template name + version |
| binary_version | string | `mp` binary semver |
| config_version | string | Hash of applied config bundle |
| policy_version | string | Hash of applied policy bundle |
| last_heartbeat | timestamp | Last check-in |
| db_size_bytes | integer | Agent DB file size |
| status | enum | active, suspended, deprovisioned |

---

## 2. Configuration-as-code and drift detection

### Problem
200 instances. A new policy needs to be pushed to all of them ("no agent shall execute `rm -rf` on production paths"). Need to know which instances are running stale config.

### Proposed tooling

```
mp fleet push-policy ./bundles/org-security-v4.json     # push to all instances
mp fleet push-policy ./bundles/team-alpha.json --scope team:alpha
mp fleet status                                          # show drift, version, health
```

- **Policy bundles** — versioned, signable JSON artifacts containing policies, behavioral rules, and trust levels. Analogous to OPA bundles.
- **Config sync** — instances pull config from a central source (Git repo, S3 bucket, internal API) on startup and periodically. Drift triggers an alert.
- **Immutable org-level policies** — policies tagged `org:enforced` that local instances cannot override or disable. Cryptographically signed; the binary verifies before loading.

### Bundle format (strawman)

```json
{
  "version": "org-security-v4",
  "signed_by": "platform-team@acme.com",
  "signature": "...",
  "policies": [
    {
      "name": "block-destructive-sql",
      "effect": "deny",
      "priority": 1000,
      "action_pattern": "*",
      "sql_pattern": "DELETE FROM .* WHERE 1|DROP TABLE|TRUNCATE",
      "message": "Destructive SQL blocked by org policy",
      "immutable": true
    }
  ],
  "behavioral_rules": [
    {
      "rule_type": "rate_limit",
      "rule_config": "{\"max\": 10, \"window_seconds\": 60}",
      "action_pattern": "call",
      "resource_pattern": "tool:shell_*"
    }
  ]
}
```

---

## 3. Secret management

### Problem
Every instance needs LLM API keys. Keys cannot live in config files or be visible to developers. They're org assets with spend implications.

### Proposed approach

- **Environment variable / secrets manager resolution** — `moneypenny.toml` references a secret name, not a value. At startup, `mp` resolves from env vars, AWS Secrets Manager, HashiCorp Vault, or 1Password CLI.
- **LLM proxy pattern** — instead of distributing raw API keys, route all LLM calls through an org-managed proxy (LiteLLM, Helicone, or custom gateway). The instance config points to `llm.internal.company.com`. The proxy handles auth, rate limiting, cost tracking, and model routing.

```toml
[agents.llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key = "vault://secret/moneypenny/anthropic-key"   # resolved at startup
# OR
base_url = "https://llm.internal.company.com/v1"       # org proxy, no key needed
```

The sidecar binary runs locally (fast, offline-capable for embeddings). The expensive/sensitive LLM calls go through the org gateway. Best of both worlds.

---

## 4. Observability and fleet-wide audit

### Problem
200 agents are running. A security incident happens. Need to know: which agents accessed production credentials? Which had policy violations in the last 24 hours? What's aggregate token spend?

### Proposed tooling

```
mp fleet audit --query "effect = 'deny'" --since 24h    # fleet-wide audit query
mp fleet audit --query "action = 'call' AND resource LIKE 'tool:shell%'" --since 7d
```

- **Audit log shipping** — each instance ships its `policy_audit` table rows to a central collector. Transport options: HTTP POST to a collector endpoint, structured log to stdout (for container log aggregation), or direct push to Datadog/Elastic/Loki.
- **Fleet dashboard** — aggregated views: policy violations/day, token spend/developer, most-denied operations, agents with stale knowledge, config drift.
- **Alerting rules:**
  - "Agent X has had 50 policy denials in the last hour" (possible prompt injection)
  - "Agent Y hasn't synced audit in 3 days" (instance may be dead)
  - "Total fleet spend exceeded daily budget"

### Shipping mechanism

The `policy_audit` table is already structured. The transport is the only gap:

```toml
[telemetry]
audit_sink = "https://audit.internal.company.com/v1/ingest"
audit_interval_seconds = 60
heartbeat_url = "https://fleet.internal.company.com/v1/heartbeat"
```

On each interval, `mp` ships new audit rows since last sync and a heartbeat with instance metadata (version, config hash, DB size, uptime).

---

## 5. Knowledge management at scale

### Problem
Org has architectural decisions, deployment runbooks, API conventions, incident response procedures. Every agent should know these. But team A's proprietary client data must never leak to team B's agent.

### Proposed approach

**Knowledge tiers:**

| Tier | Scope | Mutability | Distribution |
|------|-------|------------|--------------|
| Org | All instances | Read-only at instance level | Pushed via `mp fleet push-knowledge` |
| Team | Team's instances | Team leads can publish | CRDT sync within sync group |
| Private | Single instance | Developer-controlled | Local only, never synced |

- **Knowledge packages** — versioned bundles of documents + facts pushed to instances. Like a "knowledge release." Platform team publishes `org-knowledge-v23`, instances pull on next sync.
- **Scoped sync groups** — the CRDT sync layer already exists. The missing piece is grouping instances into sync scopes (org, team, project) with central orchestration.

```
mp fleet push-knowledge ./packages/org-runbooks-v3.tar  --scope org
mp fleet push-knowledge ./packages/team-alpha-arch.tar  --scope team:alpha
```

### Package format

```
org-runbooks-v3/
  manifest.json          # version, scope, checksums
  documents/
    incident-response.md
    deployment-guide.md
  facts/
    - content: "Production deploys happen Tuesday/Thursday via ArgoCD"
      topic: "deployment"
      confidence: 1.0
      scope: "org"
```

---

## 6. Security and isolation

### Concerns and mitigations

| Concern | Mitigation |
|---------|------------|
| Compromised agent exfiltrates data via LLM calls | LLM proxy inspects prompts; redaction runs before any outbound call |
| Agent overrides org policy to escalate privileges | Org policies are cryptographically signed; binary verifies signature |
| Lateral movement via shell_exec | Network-level isolation (container network policies); trust tiers restrict tool availability |
| Cross-tenant data leakage | SQLite-per-agent is the isolation boundary; sync groups enforce scope |
| Stale redaction patterns miss new secret formats | Central redaction rule distribution; test corpus run against each instance |
| Prompt injection tries to override governance | Policy engine runs in Rust, not in the LLM context; the agent cannot modify its own policy evaluation code path |

### Signed policy verification

```
mp fleet sign-bundle ./bundles/org-security-v4.json --key platform-team.pem
```

The `mp` binary embeds the org's public key. On startup and on policy pull, it verifies the signature. Unsigned or tampered bundles are rejected.

---

## 7. Reproducibility and disaster recovery

### Problem
Developer's laptop dies. Or need to reproduce exact agent state from last Tuesday to debug an incident.

### Proposed approach

- **Scheduled DB snapshots** — ship the SQLite file (or compressed diff) to central storage nightly. Since it's one file, this is trivially simple.
- **Point-in-time restore** — pull a snapshot, boot a new instance against it.
- **Template + seed reproducibility** — template version + knowledge package version = reproducible starting state.

```toml
[telemetry]
snapshot_sink = "s3://moneypenny-backups/${instance_id}/"
snapshot_interval = "daily"
snapshot_retain_days = 30
```

```
mp fleet restore --instance jane --date 2026-03-05 --target ./restored/
```

The one-file-is-the-whole-state property makes this dramatically simpler than backing up Postgres + Redis + Pinecone.

---

## 8. Cost governance

### Problem
200 developers x LLM calls x extraction pipeline x embeddings = real money. Some agents are more expensive than others. Some waste tokens on retry loops.

### Proposed approach

- **Per-instance token budgets** — the `token_budget` behavioral rule already exists. Fleet layer sets org-wide defaults and per-team overrides via policy bundles.
- **Central spend tracking** — LLM proxy tracks cost per instance. Dashboard shows cost/developer/day, cost by operation type.
- **Model tiering via org policy** — extraction uses cheap local 3B model, main chat uses Claude, only elevated-trust agents get the expensive model.

```json
{
  "rule_type": "token_budget",
  "rule_config": "{\"daily_limit\": 500000, \"action\": \"deny\"}",
  "scope": "org:default"
}
```

---

## Minimal tooling surface

| Command | Purpose |
|---------|---------|
| `mp fleet init --template <name>` | Create instance from template |
| `mp fleet list` | Registry of all instances with status |
| `mp fleet status` | Health, drift, version, last sync |
| `mp fleet push-policy <bundle>` | Ship policy bundle to instances |
| `mp fleet push-knowledge <package>` | Ship knowledge package to instances |
| `mp fleet audit --query <expr>` | Aggregate audit query across fleet |
| `mp fleet restore --instance <id>` | Restore instance from snapshot |
| `mp fleet sign-bundle <file>` | Sign a policy/knowledge bundle |
| `mp fleet deprovision --agent <id>` | Lifecycle offboarding |

### Supporting infrastructure

| Component | Purpose |
|-----------|---------|
| Central config repo (Git) | Version-controlled templates, policies, knowledge packages |
| Audit collector (HTTP endpoint) | Receives shipped audit logs from instances |
| LLM proxy (LiteLLM / Helicone / custom) | Central auth, cost tracking, rate limiting, prompt inspection |
| Object storage (S3 / GCS) | DB snapshots for backup and restore |
| Fleet dashboard | Aggregated observability across all instances |

---

## What already exists

The current Moneypenny architecture provides ~80% of the hard primitives:

- **Canonical operation pipeline** — every action is already policy-checked and audit-logged
- **SQLite-per-agent** — natural isolation boundary, trivially copyable
- **CRDT sync layer** — knowledge distribution primitive (needs scoped sync groups)
- **Policy engine** — static rules, behavioral rules, pattern matching (needs signed immutability)
- **Audit trail** — structured, queryable (needs transport/shipping)
- **Trust levels** — standard/elevated/admin (needs mapping to infrastructure boundaries)
- **Redaction** — 18 regex patterns (needs central distribution and verification)

## What needs to be built

1. **Fleet CLI** — `mp fleet *` commands
2. **Instance registry** — central catalog with heartbeat
3. **Bundle format + signing** — policy and knowledge packages
4. **Audit shipping** — transport layer from instance to collector
5. **DB snapshot shipping** — scheduled backup to object storage
6. **Config sync client** — pull config from central source, detect drift
7. **Scoped sync groups** — extend CRDT sync with org/team/project scoping
8. **LLM proxy integration** — config support for proxied LLM calls
