# Moneypenny — Deployment Models

This document covers the range of ways you can deploy Moneypenny, from a single-binary local tool to a multi-node replicated system.

---

## 1. Local Developer Workstation (Zero Infrastructure)

The simplest option. Run `mp init` once, then use `mp chat` or `mp send` directly from the terminal. No gateway, no daemon — just an interactive session against a local SQLite file. Use `AnthropicProvider` (or any OpenAI-compatible endpoint) for generation and the built-in `LocalEmbeddingProvider` for embeddings. DB files live on disk next to other project files.

**Best for:** Personal productivity, coding assistants, experimentation with no ops overhead.

---

## 2. Local Background Service

Run `mp start` as a persistent daemon via `launchd` (macOS), `systemd` (Linux), or a process manager like `supervisor`. The HTTP adapter (`POST /v1/chat`, SSE, WebSocket) listens on `localhost`. A reverse proxy (nginx, Caddy) adds TLS and auth on top.

For tailnet-internal access without opening ports, use **Tailscale Serve**: the gateway stays bound to loopback and Tailscale proxies HTTPS within the tailnet. This gives you signed TLS and Tailscale identity headers with no public exposure and no port forwarding. Tailscale identity can substitute for a separate auth layer in a personal setup.

**Best for:** Developer machines and home servers where you want the agent always available.

---

## 3. Remote Linux Gateway (Recommended for Most Users)

Run `mp start` on a small Linux VPS (Hetzner, Fly.io, DigitalOcean, or a home server). Clients — including a local CLI, WebChat, or channel bots — connect to the gateway remotely. Two secure access options:

- **Tailscale Serve/Funnel:** Gateway stays on loopback; Tailscale proxies connections. `serve` mode is tailnet-only; `funnel` mode is publicly reachable but requires password auth. Use `resetOnExit` to tear down the tunnel on shutdown.
- **SSH tunnel:** Forward the gateway's local port over SSH (`ssh -L 18080:localhost:18080 user@host`). No Tailscale required, works anywhere with SSH access.

The gateway host runs tools and channel connections by default. Device-local actions (camera, screen recording, system notifications, location) run on paired device nodes via `node.invoke` — so `exec` runs where the gateway lives, but device actions run where the device lives. This separation means you can run the heavy gateway on a beefy Linux box while still reaching macOS/iOS/Android peripherals.

**Best for:** Teams and individuals who want a persistent, always-on agent reachable from anywhere without a full cloud deployment.

---

## 4. Team Bot on Slack / Discord / Telegram

`mp start` on a server with one or more channel adapters configured (Slack, Discord, Telegram — all implemented). Each chat user gets session continuity via per-user session tracking. Run multiple specialized agents under one gateway (a "research" agent, a "code review" agent, etc.); the delegation tool lets them hand off work to each other.

For group/channel safety, consider a **per-session sandbox policy**: non-main sessions (groups, channels with untrusted users) run with a more restrictive policy mode (`deny_by_default`), while the main direct-message session retains full tool access. This mirrors the security model where you trust yourself but not every group participant equally.

**Best for:** Small engineering teams that want a shared assistant with persistent memory and policy-governed tool access.

---

## 5. Containerized Deployment (Docker / Kubernetes)

Package `mp start` in a Docker image. Mount a persistent volume for SQLite DB files. Pass config via a mounted `moneypenny.toml` or environment variables. The HTTP adapter is the cluster-internal service endpoint.

For multi-agent setups, the natural unit is a single container running the gateway with worker subprocesses (worker isolation already uses OS processes, which work fine inside a container). For Kubernetes, use a `StatefulSet` with a single replica per agent group rather than stateless horizontal replicas — SQLite's exclusive write lock means scaling is per-agent, not per-request.

**Best for:** Teams that want reproducible deploys, easy rollbacks, and integration with existing container infrastructure.

---

## 6. Declarative / Nix Configuration

For reproducible, version-controlled environments, a Nix flake can pin the `mp` binary and manage the `moneypenny.toml` config declaratively alongside the rest of a NixOS system or home-manager profile. This ensures the exact same binary and config across machines and eliminates install drift.

**Best for:** NixOS users or teams with existing Nix infrastructure who want the agent config treated the same as any other system configuration.

---

## 7. Fully Air-Gapped / Private Deployment

Configure both generation and embedding providers to use local GGUF models — a small generation model (Mistral 7B, Phi-3, Llama 3) via `SqliteAiProvider`, plus `nomic-embed-text-v1.5` for embeddings (ships bundled, ~274MB). Combine with:

- Encryption at rest (M16: SQLCipher or SEE, keys in platform keystore)
- `deny_by_default` policy mode
- No outbound network configuration

No traffic leaves the machine. Inference, storage, and retrieval are entirely local.

**Best for:** Enterprises with data residency requirements, government / classified use cases, or anyone who needs a hard guarantee that no data is sent to external APIs.

---

## 8. Hybrid: Local Embeddings + Cloud Generation

A distinct operational choice worth treating as its own model: use `LocalEmbeddingProvider` for all embedding and retrieval (documents and memory contents never leave the machine), but point generation at Anthropic or an OpenAI-compatible API. This avoids sending document content to an external provider during indexing and search, while keeping response quality high without running a full local generation model.

**Best for:** Cost-conscious deployments where cloud generation is acceptable but document/memory privacy matters.

---

## 9. Multi-Node Replicated (sqlite-sync — available soon)

Once `sqlite-sync` is wired in, the gateway will support CRDT-based replication of Facts, Knowledge, Skills, Policies, and Jobs across nodes. This enables:

- **Active/passive HA:** A standby node stays in sync; failover is a config switch with no data loss.
- **Geo-distributed agents:** Agents on different continents share a consistent knowledge base with eventual consistency. Each node can answer queries locally; writes propagate asynchronously.
- **Multi-agent knowledge sharing:** Agents on separate hosts share `shared`-scoped facts and knowledge without being co-located under the same gateway process.

The fact scope model (`private` / `shared` / `protected`) and policy engine already govern what replicates and what stays local — the sync layer inherits these boundaries.

**Best for:** Production deployments that need HA, multi-region presence, or knowledge federation across independently deployed agents.

---

## 10. WASM / Browser-Native (Roadmap)

Once the M17 WASM runtime lands via `sqlite-wasm`, a fully browser-native deployment becomes possible — the agent runs in-tab with no server. Storage lives in the browser's origin-private filesystem or IndexedDB. This is particularly useful for browser extensions or client-side tools where you want persistent memory without any backend.

**Best for:** Browser extensions, offline-capable web tools, demos that need zero infrastructure.

---

## Key Architectural Constraints (All Models)

- **SQLite exclusive write lock:** One gateway process per DB file. Scale horizontally by sharding per agent, not per request. `sqlite-sync` (coming soon) handles multi-node replication.
- **Worker isolation:** Each agent runs as a child OS process with exclusive DB write access. This works inside containers and on bare metal; Kubernetes `StatefulSet` is the right primitive, not `Deployment`.
- **Tailscale gateway binding:** When using Tailscale Serve or Funnel, keep `bind` on loopback. Moneypenny enforces this to prevent accidental public exposure without auth.
- **Encryption compatibility:** SQLCipher/SEE encryption is transparent to all other components. `sqlite-sync` decrypts for sync and re-encrypts at rest, so replication works across encrypted databases.
