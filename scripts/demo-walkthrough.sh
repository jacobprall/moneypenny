#!/usr/bin/env bash
# ============================================================================
# Moneypenny Feature Demo
#
# Exercises every major feature end-to-end:
#   1. Setup & init
#   2. Knowledge ingestion (file + URL)
#   3. OpenClaw history import
#   4. Structured memory (facts with 3 compression levels)
#   5. Hybrid search (FTS + vector)
#   6. Policy engine (governance, denials, audit)
#   7. Skills
#   8. Multi-agent CRDT sync
#   9. Portability (one-file copy)
#  10. Full audit trail
#  11. Raw SQL introspection
#
# Prerequisites: run ./scripts/setup.sh first (builds binary + downloads model)
#
# Usage:
#   ./scripts/demo.sh                     # run everything
#   ./scripts/demo.sh --url <URL>         # also ingest a URL
#   ./scripts/demo.sh --no-pause          # skip pauses between steps
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

PAUSE=1
INGEST_URL=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-pause) PAUSE=0; shift ;;
    --url)      INGEST_URL="${2:?--url requires a value}"; shift 2 ;;
    -h|--help)
      echo "Usage: scripts/demo.sh [--no-pause] [--url URL]"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

MP="./target/debug/mp"
DEMO_DATA="scripts/demo-data"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
STEP=0
step() {
  STEP=$((STEP + 1))
  echo ""
  printf "\033[1;36m━━━ Step %d: %s ━━━\033[0m\n" "$STEP" "$*"
  echo ""
}

run() {
  printf "\033[0;33m  \$ %s\033[0m\n" "$*"
  eval "$@" 2>&1 | sed 's/^/  /'
  echo ""
}

note() {
  printf "\033[0;90m  %s\033[0m\n" "$*"
}

pause() {
  if [[ "$PAUSE" -eq 1 ]]; then
    printf "\033[0;90m  [enter]\033[0m"
    read -r
  fi
}

sidecar_op() {
  echo "$1" | $MP sidecar 2>/dev/null
}

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------
if [[ ! -x "$MP" ]]; then
  echo "Binary not found at $MP — run ./scripts/setup.sh first"
  exit 1
fi

if [[ ! -f "$DEMO_DATA/project-handbook.md" ]]; then
  echo "Demo data missing at $DEMO_DATA/"
  exit 1
fi

# Preserve embedding model across reinit (it's ~140MB, don't re-download)
if [[ -f "mp-data/models/nomic-embed-text-v1.5.gguf" ]]; then
  cp "mp-data/models/nomic-embed-text-v1.5.gguf" /tmp/_mp_demo_model.gguf
fi

echo ""
echo "  ┌──────────────────────────────────────┐"
echo "  │     Moneypenny — Full Feature Demo   │"
echo "  └──────────────────────────────────────┘"

# ===================================================================
# 1. Clean slate
# ===================================================================
step "Clean slate — init from scratch"

rm -rf mp-data moneypenny.toml

run "$MP init"

if [[ -f /tmp/_mp_demo_model.gguf ]]; then
  mv /tmp/_mp_demo_model.gguf mp-data/models/nomic-embed-text-v1.5.gguf
  note "Restored embedding model into mp-data/models/"
  echo ""
elif [[ ! -f "mp-data/models/nomic-embed-text-v1.5.gguf" ]]; then
  note "No embedding model found. Vector search will be skipped."
  note "Run ./scripts/setup.sh first for full vector search support."
  echo ""
fi

run "$MP health"

pause

# ===================================================================
# 2. Knowledge ingestion — local file
# ===================================================================
step "Ingest a document into the knowledge base"

run "$MP ingest '$DEMO_DATA/project-handbook.md'"
run "$MP knowledge list"

pause

# ===================================================================
# 3. Knowledge ingestion — URL (optional)
# ===================================================================
if [[ -n "$INGEST_URL" ]]; then
  step "Ingest from URL"

  run "$MP ingest --url '$INGEST_URL'"
  run "$MP knowledge list"

  pause
fi

# ===================================================================
# 4. OpenClaw history import
# ===================================================================
step "Import OpenClaw conversation history"

run "$MP ingest --openclaw-file '$DEMO_DATA/openclaw-history.jsonl' --source openclaw"
run "$MP ingest --status --source openclaw"

pause

# ===================================================================
# 5. Structured memory — facts with 3 compression levels
# ===================================================================
step "Add structured facts (3 compression levels: full / summary / pointer)"

note "Injecting facts via the sidecar canonical-op interface (no LLM needed)."
note "Each fact has: content (full detail), summary (mid), pointer (2-5 word tag)."
echo ""

sidecar_op '{"op":"memory.fact.add","args":{"content":"The deployment pipeline uses ArgoCD with lint, unit tests, integration tests, canary (5% traffic for 30 minutes), then full rollout. Deploys happen Tue/Thu. Rollbacks auto-trigger if error rate exceeds 0.1%. Emergency hotfixes bypass canary with VP approval.","summary":"ArgoCD pipeline: lint-test-canary(5%/30min)-rollout Tue/Thu, auto-rollback at 0.1%","pointer":"DEPLOY: argo-pipeline-tue-thu","confidence":0.95,"keywords":"deployment argocd canary rollback pipeline"}}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  fact added: {d[\"data\"].get(\"id\",\"?\")}')" 2>/dev/null

sidecar_op '{"op":"memory.fact.add","args":{"content":"Fleet of 200 autonomous warehouse robots across Austin, Chicago, and Newark. Four core services: Navigator (LiDAR path planning), Picker (6-DOF arm), Orchestrator (fleet coordination), Vision (YOLOv8 on edge TPUs). All communicate over gRPC with Protocol Buffers.","summary":"200 robots, 3 sites, 4 services (Navigator/Picker/Orchestrator/Vision) over gRPC","pointer":"FLEET: 200-robots-3-sites-4-svcs","confidence":0.92,"keywords":"robots fleet architecture navigator picker orchestrator vision"}}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  fact added: {d[\"data\"].get(\"id\",\"?\")}')" 2>/dev/null

sidecar_op '{"op":"memory.fact.add","args":{"content":"Performance metrics: fulfillment rate 99.7% (target 99.9%), mean pick time 4.2s (target 3.5s), robot uptime 98.1% (target 99.5%), MTTR 12min (target 10min). Main bottleneck is arm trajectory planning.","summary":"Metrics below target: pick 4.2s/3.5s, uptime 98.1%/99.5%, MTTR 12/10min","pointer":"PERF: metrics-below-target","confidence":0.88,"keywords":"metrics performance pick time uptime bottleneck"}}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  fact added: {d[\"data\"].get(\"id\",\"?\")}')" 2>/dev/null

sidecar_op '{"op":"memory.fact.add","args":{"content":"Security: mTLS with 90-day cert rotation, dual-signature firmware updates (engineering + security), PII processed on-device only, production DB requires MFA, quarterly external pentests.","summary":"mTLS+90d rotation, dual-sig firmware, on-device PII, MFA prod, quarterly pentests","pointer":"SEC: mtls-dualsig-ondevice-pii","confidence":0.97,"keywords":"security mtls firmware pii mfa penetration testing"}}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  fact added: {d[\"data\"].get(\"id\",\"?\")}')" 2>/dev/null

sidecar_op '{"op":"memory.fact.add","args":{"content":"Top priorities: 1) Reduce pick time 4.2s to 3.5s via trajectory optimization, 2) Roll out firmware v3.2 to all sites, 3) Complete SOC 2 Type II by end of Q2, 4) Hire 3 senior perception engineers, 5) Migrate CockroachDB to TiKV for 40% cost savings.","summary":"Top 5: pick time opt, fw v3.2 rollout, SOC2 Q2, hire 3 eng, CRDB-to-TiKV","pointer":"PRIORITIES: pick-fw-soc2-hire-tikv","confidence":0.90,"keywords":"priorities firmware soc2 hiring tikv migration"}}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  fact added: {d[\"data\"].get(\"id\",\"?\")}')" 2>/dev/null

echo ""
note "Listing facts — the pointer column shows the compact 2-5 word tags:"
echo ""

run "$MP facts list"

pause

# ===================================================================
# 6. Hybrid search
# ===================================================================
step "Hybrid search across facts + knowledge"

note "facts search uses memory.search (FTS + vector). knowledge search uses FTS5."
echo ""

run "$MP facts search 'ArgoCD'"
run "$MP facts search 'bottleneck'"
run "$MP knowledge search 'security'"

pause

# ===================================================================
# 7. Policy engine
# ===================================================================
step "Policy engine — add rules, test governance"

note "Adding a deny rule to block destructive SQL..."
echo ""

sidecar_op '{"op":"policy.add","args":{"name":"no-destructive-sql","effect":"deny","actor_pattern":"*","action_pattern":"execute","resource_pattern":"sql:*DROP*","message":"Destructive SQL is blocked"}}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  policy added: {d[\"data\"].get(\"id\",\"?\")}')" 2>/dev/null

note "Adding an audit rule to log all memory searches..."
echo ""

sidecar_op '{"op":"policy.add","args":{"name":"audit-searches","effect":"audit","actor_pattern":"*","action_pattern":"search","resource_pattern":"memory"}}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  policy added: {d[\"data\"].get(\"id\",\"?\")}')" 2>/dev/null

run "$MP policy list"
run "$MP policy test 'DROP TABLE facts'"
run "$MP policy test 'SELECT * FROM facts'"

pause

# ===================================================================
# 8. Skills
# ===================================================================
step "Register a reusable skill"

sidecar_op '{"op":"skill.add","args":{"name":"incident-triage","description":"Triage production incidents using the escalation runbook","content":"When triaging: 1) Check dashboards for scope. 2) Escalation: on-call - team lead - VP Eng - CTO. 3) Single-robot: SRE investigates. 4) Fleet-wide: escalate to platform lead immediately. 5) Document in incident channel. 6) Post-incident review within 48h."}}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'  skill added: {d.get(\"code\",\"?\")}')" 2>/dev/null

echo ""
run "$MP skill list"

pause

# ===================================================================
# 9. Multi-agent CRDT sync
# ===================================================================
step "Multi-agent CRDT sync"

run "$MP sync status"

note "Creating a second agent (research) and syncing facts to it..."
echo ""

run "$MP agent create research"

note "Before sync — count facts in each DB:"
MAIN_COUNT=$($MP db query "SELECT count(*) FROM facts WHERE status='active'" 2>/dev/null | tail -1 | tr -d ' ')
echo "  main.db:     $MAIN_COUNT active facts"
RESEARCH_DB="mp-data/research.db"
if command -v sqlite3 >/dev/null 2>&1; then
  RESEARCH_COUNT=$(sqlite3 "$RESEARCH_DB" "SELECT count(*) FROM facts WHERE status='active';" 2>/dev/null || echo "0")
else
  RESEARCH_COUNT="0"
fi
echo "  research.db: $RESEARCH_COUNT active facts"
echo ""

run "$MP sync push --to '$RESEARCH_DB'"

note "After sync — research now has main's facts:"
if command -v sqlite3 >/dev/null 2>&1; then
  SYNCED_COUNT=$(sqlite3 "$RESEARCH_DB" "SELECT count(*) FROM facts WHERE status='active';" 2>/dev/null || echo "0")
  echo "  research.db: $SYNCED_COUNT active facts"
  echo ""
  printf "\033[0;33m  \$ sqlite3 research.db \"SELECT pointer FROM facts WHERE status='active'\"\033[0m\n"
  sqlite3 "$RESEARCH_DB" "SELECT '  ' || pointer FROM facts WHERE status='active';" 2>/dev/null || echo "  (query failed)"
  echo ""
fi

pause

# ===================================================================
# 10. Portability — one-file copy
# ===================================================================
step "Portability — one file IS the agent"

DBSIZE=$(wc -c < mp-data/main.db | tr -d ' ')
echo "  mp-data/main.db = $(python3 -c "print(f'{$DBSIZE / 1024:.0f} KB')")"
echo ""
echo "  That single file contains:"

if command -v sqlite3 >/dev/null 2>&1; then
  sqlite3 mp-data/main.db "
    SELECT '    ' || count(*) || ' facts' FROM facts WHERE status='active'
    UNION ALL
    SELECT '    ' || count(*) || ' knowledge chunks' FROM chunks
    UNION ALL
    SELECT '    ' || count(*) || ' skills' FROM skills
    UNION ALL
    SELECT '    ' || count(*) || ' policies' FROM policies
    UNION ALL
    SELECT '    ' || count(*) || ' audit entries' FROM policy_audit
    UNION ALL
    SELECT '    ' || count(*) || ' external events' FROM external_events;
  "
else
  run "$MP db query \"SELECT 'facts: ' || count(*) FROM facts WHERE status='active'\""
fi

echo ""
echo "  Copy it, back it up, move it, open it with any SQLite client."

pause

# ===================================================================
# 11. Audit trail
# ===================================================================
step "Audit trail — every operation is recorded"

run "$MP audit"

pause

# ===================================================================
# 12. Raw SQL introspection
# ===================================================================
step "SQL introspection — the database IS the runtime"

run "$MP db query \"SELECT id, pointer, confidence FROM facts WHERE status='active' ORDER BY confidence DESC\""
run "$MP db query \"SELECT id, title, chunk_count FROM documents\""
run "$MP db query \"SELECT name, effect, actor_pattern, resource_pattern FROM policies ORDER BY priority DESC LIMIT 5\""
run "$MP db query \"SELECT source, count(*) as events FROM external_events GROUP BY source\""

pause

# ===================================================================
# 13. Final status
# ===================================================================
step "Final status"

run "$MP agent status"
run "$MP health"

echo ""
echo "  ┌──────────────────────────────────────────────────────┐"
echo "  │  Demo complete.                                      │"
echo "  │                                                      │"
echo "  │  Features exercised:                                 │"
echo "  │    Knowledge ingestion (file + URL)                  │"
echo "  │    OpenClaw history import with projection           │"
echo "  │    Structured facts (3 compression levels)           │"
echo "  │    Hybrid search (FTS5 + vector)                     │"
echo "  │    Policy governance (deny + audit rules)            │"
echo "  │    Reusable skills                                   │"
echo "  │    Multi-agent CRDT sync                             │"
echo "  │    One-file portability                              │"
echo "  │    Full audit trail                                  │"
echo "  │    SQL-level introspection                           │"
echo "  │                                                      │"
echo "  │  All local. No cloud. One SQLite file per agent.     │"
echo "  └──────────────────────────────────────────────────────┘"
echo ""
