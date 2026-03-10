---
title: "Build Brains, Not Bots"
description: "Code is becoming a commodity. Agents will write it. But agents without memory, governance, or coordination are just stochastic processes. The missing ingredient isn't better models — it's persistent state."
date: 2026-03-09
author: "Jacob Prall"
tags: ["thesis", "moneypenny"]
---

Code is becoming a commodity.

Specs define intent. Tests define correctness. Between those two boundaries, the act of writing code is mechanical. Pattern matching against known solutions, translating requirements into syntax, wiring components that have been wired a thousand times before. Coding agents are already good at this. They will only get better.

But coding agents without context, controls, or coordination aren't much better than monkeys and typewriters.

Today, coding agents have no memory. They don't know what was built yesterday, what patterns the team prefers, or that the last three attempts to refactor the billing service failed for the same reason.

Coding agents have no focus. Context engineering is the laborious workaround — manually curating what fits inside a fixed aperture. Fit everything relevant in, or lose it.

Coding agents don't have colleagues. Deploy ten agents across a codebase and they are ten strangers in the same building. No shared understanding, no coordination mechanism, no way to surface what one agent learned to another. Shared state requires heavy cloud infrastructure. Usually, it doesn't exist at all.

Coding agents have no accountability. No permissions. Nothing to stop one from running DROP TABLE at 3am. No queryable audit trail, either. Nothing to explain why it was stopped, or what to learn from the stopping.

Coding agents have no tools that last. You build a deployment procedure, a triage workflow, a custom API client — and it dies with the context window. The next session starts from nothing. Capability doesn't accumulate. It evaporates.

Strip away the impressive code generation and what remains is a stochastic process. One that produces useful output often enough to be exciting — but with no memory to ground it, no boundaries to guide it, no tools to reach for, and no mechanism to share what it learns.

The missing ingredient isn't better models. It's persistent state — something that remembers across sessions, governs what agents can do, accumulates tools, and syncs it all across a fleet. Not a bigger context window. A brain.

---

## Moneypenny

Moneypenny is a brain for coding agents.

It deploys alongside any CLI agent — Claude Code, Cursor, or whatever comes next — as a sidecar process over MCP. One command to connect. From that point forward, every conversation turn passes through a persistent intelligence layer backed by a single SQLite file.

The architecture is simple. The database is the runtime.

**Memory.** After every turn, an extraction pipeline distills knowledge into structured facts. Each fact has three compression levels: full content, summary, and a 2–5 word pointer. All facts load as pointers by default. Only what matters expands. Confidence grows on re-extraction. Stale knowledge decays. The agent's memory self-curates.

**Search.** Facts, documents, conversation history, and session scratch feed into a single hybrid retrieval layer — vector similarity plus full-text search, fused via Reciprocal Rank Fusion, deduplicated with diversity ranking. When an agent hits a bug, it searches in natural language across everything the team knows. Patterns surface. Problems pinpoint.

**Governance.** A policy engine evaluates every tool call, every memory write, every SQL query before execution. Static rules block destructive operations. Behavioral rules rate-limit shell access, detect retry loops, enforce token budgets. Denials don't crash the agent — they're returned as context so it can adapt. Every decision is logged. The audit trail is queryable.

**Tools.** JavaScript tools and reusable procedures stored in the database. A deployment checklist, a triage workflow, a code review protocol, a custom API client. Create a tool for one agent and it syncs to every brain in the fleet. Tools track usage and success rate — high performers surface more in retrieval. They're governed by the same policy engine.

**Sync.** Agent databases replicate via CRDTs — conflict-free replicated data types that merge without a central server. Facts, tools, policies, and fact links sync across agents. Conversations stay local. Scoping controls what flows where: private, shared, or protected. The sync layer doesn't just share knowledge — it distributes the full behavioral envelope. When a new policy or tool arrives via sync, the agent's capabilities and constraints update in place.

**Jobs.** Cron-scheduled tasks — reflection prompts, pipeline checks, metric sweeps — governed by the same policy engine. Define once, propagate across the fleet.

One file is the whole agent. Back it up by copying it. Move it with scp. Inspect it with sqlite3. One file, one brain.

---

## The fleet

Now multiply.

Two hundred agents, each assigned a domain — payments, auth, search, infrastructure, onboarding. Each accumulates knowledge. The payments agent knows the retry logic was rewritten in January for idempotency keys. The search agent knows the Elasticsearch cluster has an undocumented 5-second timeout.

These facts sync. When the api-gateway agent rewrites the search endpoint, it already knows about the timeout — not because someone pasted it into the prompt, but because the search agent's knowledge flowed through the sync layer, arrived as a structured fact, and expanded when the context demanded it.

Shared memory is coordination. The synced row is the message. The database schema is the protocol.

A platform engineer manages this fleet the way they manage infrastructure today. Templates for provisioning. Policy bundles pushed to all instances. Audit logs shipped to a central collector.

```
mp fleet init --template backend-engineer
mp fleet push-policy ./bundles/org-security-v4.json
mp fleet push-knowledge ./packages/runbooks-v3.tar --scope team:infra
mp fleet audit --query "effect = 'deny'" --since 24h
```

The unit of state is a file. The unit of governance is a policy. The unit of coordination is a fact. The unit of capability is a tool. All four sync.

---

Specs in, features out. That future requires brains. The code was never the hard part.

*Moneypenny is open source. [Apache-2.0](https://github.com/jacobprall/moneypenny).*
