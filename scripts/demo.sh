#!/usr/bin/env bash
# ============================================================================
# Moneypenny — Interactive Demo Setup
#
# Creates a rich demo environment from scratch, then drops you into
# interactive mode. Designed for recordings and live demos.
#
# What it sets up:
#   - 3 agents (main, research, ops-bot) with distinct personas
#   - 4 knowledge documents (handbook, runbook, API ref, Q1 retro)
#   - 15 structured facts across deployment, architecture, security, metrics
#   - 6 governance policies (deny, audit, rate-limit)
#   - 2 skills (incident triage, deploy verification)
#   - 1 scheduled job (daily metrics check)
#   - OpenClaw history import
#   - CRDT sync between agents
#   - Full audit trail
#
# Prerequisites:
#   ./scripts/setup.sh
#
# Usage:
#   ./scripts/demo.sh                  # setup + interactive cheat sheet
#   ./scripts/demo.sh --chat           # setup + drop into mp chat
#   ./scripts/demo.sh --quiet          # suppress setup progress
#   ./scripts/demo.sh --skip-setup     # skip setup, show cheat sheet only
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

AUTO_CHAT=0
QUIET=0
SKIP_SETUP=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --chat)       AUTO_CHAT=1; shift ;;
    --quiet)      QUIET=1; shift ;;
    --skip-setup) SKIP_SETUP=1; shift ;;
    -h|--help)
      echo "Usage: scripts/demo.sh [--chat] [--quiet] [--skip-setup]"
      echo ""
      echo "  --chat        Drop into mp chat after setup"
      echo "  --quiet       Suppress progress output during setup"
      echo "  --skip-setup  Skip setup, just show the interactive cheat sheet"
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

info() {
  [[ "$QUIET" -eq 1 ]] && return
  STEP=$((STEP + 1))
  printf "\033[1;36m[%d/10]\033[0m %s " "$STEP" "$*"
}

done_msg() {
  [[ "$QUIET" -eq 1 ]] && return
  printf "\033[1;32m✓\033[0m\n"
}

fail_msg() {
  printf "\033[1;31m✗ %s\033[0m\n" "$*"
  exit 1
}

sidecar() {
  echo "$1" | $MP sidecar 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------
[[ -x "$MP" ]] || fail_msg "Binary not found at $MP — run ./scripts/setup.sh first"

for f in project-handbook.md incident-runbook.md api-reference.md q1-retrospective.md openclaw-history.jsonl; do
  [[ -f "$DEMO_DATA/$f" ]] || fail_msg "Missing demo data: $DEMO_DATA/$f"
done

if [[ "$SKIP_SETUP" -eq 1 ]]; then
  STEP=10
else

# Preserve embedding model
MODEL_BAK=""
if [[ -f "mp-data/models/nomic-embed-text-v1.5.gguf" ]]; then
  MODEL_BAK=$(mktemp)
  cp "mp-data/models/nomic-embed-text-v1.5.gguf" "$MODEL_BAK"
fi

[[ "$QUIET" -eq 0 ]] && echo ""
[[ "$QUIET" -eq 0 ]] && echo "  Setting up Moneypenny demo environment..."
[[ "$QUIET" -eq 0 ]] && echo ""

# ===================================================================
# 1. Clean init
# ===================================================================
info "Initializing fresh database"

rm -rf mp-data moneypenny.toml
$MP init >/dev/null 2>&1

if [[ -n "$MODEL_BAK" && -f "$MODEL_BAK" ]]; then
  mv "$MODEL_BAK" mp-data/models/nomic-embed-text-v1.5.gguf
fi

done_msg

# ===================================================================
# 2. Create agents with personas
# ===================================================================
info "Creating agents (main, research, ops-bot)"

$MP agent create research >/dev/null 2>&1
$MP agent create ops-bot >/dev/null 2>&1

$MP agent config main persona "You are a senior engineering assistant for Acme Robotics. You know the team's architecture, deployment pipeline, security policies, and current priorities. You are thorough, cite facts from memory, and flag risks proactively." >/dev/null 2>&1
$MP agent config research persona "You are a research analyst for Acme Robotics. You investigate technical questions deeply, compare alternatives, and produce structured reports with pros/cons/recommendations." >/dev/null 2>&1
$MP agent config ops-bot persona "You are an SRE bot for Acme Robotics. You monitor fleet health, triage incidents using the runbook, and coordinate response. You are concise, action-oriented, and always cite the severity level." >/dev/null 2>&1

done_msg

# ===================================================================
# 3. Ingest knowledge documents
# ===================================================================
info "Ingesting 4 knowledge documents"

$MP ingest "$DEMO_DATA/project-handbook.md" >/dev/null 2>&1
$MP ingest "$DEMO_DATA/incident-runbook.md" >/dev/null 2>&1
$MP ingest "$DEMO_DATA/api-reference.md" >/dev/null 2>&1
$MP ingest "$DEMO_DATA/q1-retrospective.md" >/dev/null 2>&1

done_msg

# ===================================================================
# 4. Import OpenClaw history
# ===================================================================
info "Importing OpenClaw conversation history"

$MP ingest --openclaw-file "$DEMO_DATA/openclaw-history.jsonl" --source openclaw >/dev/null 2>&1

done_msg

# ===================================================================
# 5. Load structured facts
# ===================================================================
info "Loading 15 structured facts"

# -- Deployment & Architecture --
sidecar '{"op":"memory.fact.add","args":{"content":"The deployment pipeline uses ArgoCD with lint, unit tests, integration tests, canary (5% traffic for 30 minutes), then full rollout. Deploys happen Tue/Thu. Rollbacks auto-trigger if error rate exceeds 0.1%. Emergency hotfixes bypass canary with VP approval.","summary":"ArgoCD pipeline: lint-test-canary(5%/30min)-rollout Tue/Thu, auto-rollback at 0.1%","pointer":"DEPLOY: argo-pipeline-tue-thu","confidence":0.95,"keywords":"deployment argocd canary rollback pipeline tuesday thursday"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"Fleet of 200 autonomous warehouse robots across Austin, Chicago, and Newark. Four core services: Navigator (LiDAR path planning), Picker (6-DOF arm), Orchestrator (fleet coordination), Vision (YOLOv8 on edge TPUs). All communicate over gRPC with Protocol Buffers.","summary":"200 robots, 3 sites, 4 services (Navigator/Picker/Orchestrator/Vision) over gRPC","pointer":"FLEET: 200-robots-3-sites-4-svcs","confidence":0.95,"keywords":"robots fleet architecture navigator picker orchestrator vision grpc"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"The Orchestrator service was rewritten from Go to Rust in Q1 2026. P99 latency dropped from 180ms to 12ms. This validated the decision to use Rust for all new services going forward.","summary":"Orchestrator Go→Rust rewrite: P99 180ms→12ms, Rust for all new services","pointer":"ARCH: orchestrator-rust-rewrite","confidence":0.92,"keywords":"orchestrator rust go rewrite latency performance"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"API versioning policy: semantic versioning, breaking changes require 90-day deprecation notice, new fields are backwards compatible. Current versions: Navigator v2.4, Picker v2.1, Orchestrator v3.0, Vision v1.8.","summary":"SemVer APIs, 90-day deprecation, Nav v2.4 / Pick v2.1 / Orch v3.0 / Vis v1.8","pointer":"API: versioning-policy","confidence":0.90,"keywords":"api versioning semver deprecation navigator picker orchestrator vision"}}' >/dev/null

# -- Performance & Metrics --
sidecar '{"op":"memory.fact.add","args":{"content":"Performance metrics Q1 2026: fulfillment rate 99.7% (target 99.9%), mean pick time 4.2s (target 3.5s), robot uptime 98.1% (target 99.5%), MTTR 12min (target 10min). Main bottleneck is arm trajectory planning in tight Newark shelf spacing.","summary":"Metrics below target: pick 4.2s/3.5s, uptime 98.1%/99.5%, MTTR 12/10min","pointer":"PERF: q1-metrics-below-target","confidence":0.88,"keywords":"metrics performance pick time uptime bottleneck newark fulfillment"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"Mean pick time regressed from 3.8s to 4.2s after Newark launch. Root cause: Newark shelf spacing is 15% narrower than Austin/Chicago. The inverse kinematics solver generates conservative trajectories for tight spaces. Fix requires retraining the trajectory model on Newark-specific geometry.","summary":"Pick time regression 3.8s→4.2s caused by Newark narrow shelves, needs model retrain","pointer":"PERF: pick-time-newark-regression","confidence":0.90,"keywords":"pick time regression newark shelves inverse kinematics trajectory"}}' >/dev/null

# -- Security --
sidecar '{"op":"memory.fact.add","args":{"content":"Security posture: mTLS with 90-day cert rotation, dual-signature firmware updates (engineering + security), PII processed on-device only (edge-first vision), production DB requires MFA, quarterly external pentests. SOC 2 Type II audit in progress — 14/18 controls done.","summary":"mTLS+90d, dual-sig firmware, on-device PII, MFA prod, SOC2 14/18 controls done","pointer":"SEC: posture-and-soc2-progress","confidence":0.97,"keywords":"security mtls firmware pii mfa penetration testing soc2 audit"}}' >/dev/null

# -- Priorities & Decisions --
sidecar '{"op":"memory.fact.add","args":{"content":"Top priorities Q2 2026: 1) Reduce pick time 4.2s→3.5s via trajectory optimization, 2) Complete SOC 2 Type II audit, 3) Finish TiKV migration (target end of May), 4) Hire 3 senior perception engineers, 5) Ship firmware v3.2 to all sites, 6) Implement automated canary analysis, 7) Reduce MTTR 12min→10min.","summary":"Q2: pick opt, SOC2, TiKV migration, hire 3, fw v3.2, auto canary, MTTR","pointer":"PRIORITIES: q2-2026-top-7","confidence":0.90,"keywords":"priorities firmware soc2 hiring tikv migration canary mttr q2"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"Key architecture decision: all new services will be written in Rust. Go services migrated opportunistically. Rationale: Orchestrator rewrite proved 15x latency improvement with better memory safety. Decision made Q1 2026.","summary":"All new services in Rust (Go migrated opportunistically), decided Q1 2026","pointer":"DECISION: rust-for-new-services","confidence":0.93,"keywords":"rust go architecture decision language services"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"Database migration from CockroachDB to TiKV is 40% complete (read path done, write path in progress). Was blocked 3 weeks by TiKV replication bug #14892 — Acme contributed a fix upstream, merged March 15. Revised completion: end of May 2026.","summary":"CockroachDB→TiKV 40% done, blocked by #14892 (fixed), target May 2026","pointer":"MIGRATION: crdb-to-tikv-40pct","confidence":0.85,"keywords":"cockroachdb tikv migration database replication"}}' >/dev/null

# -- Team & Operations --
sidecar '{"op":"memory.fact.add","args":{"content":"Team structure: Platform (8 eng, owns Navigator+Orchestrator), Manipulation (5 eng, owns Picker+gripper), Perception (6 eng, owns Vision+sensors), SRE (4 eng, owns infra+monitoring). On-call rotation weekly, shared across teams.","summary":"4 teams (Platform 8, Manipulation 5, Perception 6, SRE 4), weekly on-call","pointer":"TEAM: structure-and-oncall","confidence":0.92,"keywords":"team structure platform manipulation perception sre on-call"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"Newark facility launched Feb 3 2026, two weeks ahead of schedule. Zero SEV1 incidents in first 30 days. Running at 97.2% uptime (above 95% ramp target). Fleet expanded from 130 to 200 robots.","summary":"Newark launched Feb 3, ahead of schedule, 0 SEV1s, 97.2% uptime, fleet→200","pointer":"SITE: newark-launch-success","confidence":0.90,"keywords":"newark launch facility robots uptime"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"Hiring: Q1 target was 3 senior perception engineers. Hired 0. Lost two finalists to Waymo and Figure. Adjusted comp bands in March — two offers outstanding for April start dates.","summary":"Perception hiring 0/3 in Q1, lost to Waymo/Figure, 2 offers out for April","pointer":"HIRING: perception-eng-behind","confidence":0.80,"keywords":"hiring perception engineers waymo figure compensation"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"Weekly architecture review every Tuesday at 10am. Mandatory for team leads, optional for ICs. Decisions documented in #arch-decisions Slack channel.","summary":"Arch review: Tue 10am, mandatory leads, decisions in #arch-decisions","pointer":"PROCESS: weekly-arch-review","confidence":0.88,"keywords":"architecture review meeting tuesday decisions"}}' >/dev/null

sidecar '{"op":"memory.fact.add","args":{"content":"Firmware v3.1 reduced LiDAR false positives by 60%. Firmware v3.2 with improved LiDAR processing is ready for rollout to all three sites. Firmware updates require dual-signature verification (engineering + security team).","summary":"FW v3.1: -60% LiDAR false pos. v3.2 ready for rollout (dual-sig required)","pointer":"FIRMWARE: v3.1-results-v3.2-pending","confidence":0.90,"keywords":"firmware lidar false positives rollout dual signature"}}' >/dev/null

done_msg

# ===================================================================
# 6. Add policies
# ===================================================================
info "Configuring 6 governance policies"

sidecar '{"op":"policy.add","args":{"name":"no-destructive-sql","effect":"deny","actor_pattern":"*","action_pattern":"execute","resource_pattern":"sql:*DROP*","message":"Destructive SQL (DROP) is blocked by policy"}}' >/dev/null
sidecar '{"op":"policy.add","args":{"name":"no-delete-without-where","effect":"deny","actor_pattern":"*","action_pattern":"execute","resource_pattern":"sql:*","sql_pattern":"^DELETE\\s+FROM\\s+\\w+\\s*$","message":"DELETE without WHERE clause is prohibited"}}' >/dev/null
sidecar '{"op":"policy.add","args":{"name":"audit-all-searches","effect":"audit","actor_pattern":"*","action_pattern":"search","resource_pattern":"memory","message":"Memory search logged for audit"}}' >/dev/null
sidecar '{"op":"policy.add","args":{"name":"no-shell-for-standard","effect":"deny","actor_pattern":"*","action_pattern":"call","resource_pattern":"shell_exec","message":"Shell access requires elevated trust. Ask an admin."}}' >/dev/null
sidecar '{"op":"policy.add","args":{"name":"audit-file-writes","effect":"audit","actor_pattern":"*","action_pattern":"call","resource_pattern":"file_write","message":"File write operations are audited"}}' >/dev/null
sidecar '{"op":"policy.add","args":{"name":"rate-limit-web-search","effect":"deny","actor_pattern":"*","action_pattern":"call","resource_pattern":"web_search","rule_type":"rate_limit","rule_config":"{\"max_calls\":10,\"window_secs\":60}","message":"Web search rate limited to 10 calls per minute"}}' >/dev/null

done_msg

# ===================================================================
# 7. Register skills
# ===================================================================
info "Registering 2 skills"

sidecar '{"op":"skill.add","args":{"name":"incident-triage","description":"Triage production incidents using the Acme Robotics escalation runbook","content":"## Incident Triage Procedure\n\n1. **Classify severity** — SEV1: fleet-wide / >10% offline (5min response). SEV2: single-site degradation (15min). SEV3: single robot (1hr). SEV4: cosmetic (next day).\n2. **Escalation path** — On-call → Team lead → VP Eng → CTO.\n3. **For SEV1** — Page via PagerDuty, open #inc-YYYY-MM-DD channel, assign IC, status updates every 15min.\n4. **Common failures** — Robot won'\''t move: check Navigator logs + LiDAR + firmware. Arm failure: check Picker + IK solver + sensor calibration. Fleet coordination lost: SEV1, check Orchestrator + CockroachDB + Kafka lag.\n5. **Post-incident** — PIR within 48h, action items in Linear with incident label."}}' >/dev/null

sidecar '{"op":"skill.add","args":{"name":"deploy-verification","description":"Verify a production deployment is healthy","content":"## Deploy Verification Checklist\n\n1. Confirm ArgoCD sync status is Healthy\n2. Check error rate in Grafana (should be < 0.1%)\n3. Verify canary traffic split (5% for 30 minutes)\n4. Monitor P99 latency — should not regress > 10% from baseline\n5. Check CockroachDB replication lag < 100ms\n6. Verify Kafka consumer lag < 1 minute\n7. Spot-check 3 robots per site: position reporting, pick success, battery level\n8. If all green after 30 minutes: approve full rollout\n9. If any metric breached: trigger automatic rollback\n10. Post-deploy: update #deployments channel with status"}}' >/dev/null

done_msg

# ===================================================================
# 8. Create a scheduled job
# ===================================================================
info "Creating scheduled job (daily metrics check)"

sidecar '{"op":"job.create","args":{"name":"daily-metrics-review","schedule":"0 9 * * *","job_type":"prompt","description":"Daily fleet performance metrics review","payload":"{\"prompt\":\"Review current fleet performance metrics. Compare fulfillment rate, mean pick time, robot uptime, and MTTR against targets. Flag any regressions and suggest actions.\"}"}}' >/dev/null

done_msg

# ===================================================================
# 9. Sync facts to research and ops-bot
# ===================================================================
info "Syncing facts across agents (CRDT)"

$MP sync push --to mp-data/research.db >/dev/null 2>&1 || true
$MP sync push --to mp-data/ops-bot.db >/dev/null 2>&1 || true

done_msg

# ===================================================================
# 10. Promote some facts to shared scope
# ===================================================================
info "Promoting key facts to shared scope"

# Promote facts whose pointer starts with DEPLOY, SEC, or PRIORITIES
while IFS= read -r fid; do
  fid=$(echo "$fid" | tr -d '[:space:]|')
  [[ -n "$fid" && "$fid" != "id" && "$fid" != "--" ]] && \
    sidecar "{\"op\":\"memory.fact.update\",\"args\":{\"id\":\"$fid\",\"scope\":\"shared\"}}" >/dev/null 2>&1 || true
done < <($MP db query "SELECT id FROM facts WHERE superseded_at IS NULL AND (pointer LIKE 'DEPLOY:%' OR pointer LIKE 'SEC:%' OR pointer LIKE 'PRIORITIES:%')" 2>/dev/null || true)

done_msg

# end of SKIP_SETUP guard
fi

# ===================================================================
# Summary + Interactive Cheat Sheet
# ===================================================================
echo ""
echo "  ┌──────────────────────────────────────────────────────────────┐"
echo "  │            Moneypenny Demo Environment Ready                 │"
echo "  └──────────────────────────────────────────────────────────────┘"
echo ""

if [[ "$SKIP_SETUP" -eq 0 ]]; then
  # Show what was loaded
  # count data rows (skip header + separator lines)
  FACT_COUNT=$($MP facts list 2>/dev/null | tail -n +3 | grep -v '^\s*$' | wc -l | tr -d ' ')
  DOC_COUNT=$($MP knowledge list 2>/dev/null | tail -n +3 | grep -v '^\s*$' | wc -l | tr -d ' ')
  POLICY_COUNT=$($MP policy list 2>/dev/null | tail -n +3 | grep -v '^\s*$' | wc -l | tr -d ' ')
  SKILL_COUNT=$($MP skill list 2>/dev/null | tail -n +3 | grep -v '^\s*$' | wc -l | tr -d ' ')

  printf "  \033[0;90mLoaded: %s facts · %s docs · %s policies · %s skills · 3 agents · 1 job\033[0m\n" \
    "$FACT_COUNT" "$DOC_COUNT" "$POLICY_COUNT" "$SKILL_COUNT"
  echo ""
fi

cat <<'CHEATSHEET'
  ─── MEMORY & FACTS ──────────────────────────────────────────────
  CLI                                   Natural Language (MCP)
  ─────────────────────────────────     ──────────────────────────────
  mp facts list                         "Show me everything you know"
  mp facts search "pick time"           "What do you know about pick time?"
  mp facts search "security"            "Summarize our security posture"
  mp knowledge search "escalation"      "How do we handle incidents?"

  ─── SEARCH & RETRIEVAL ──────────────────────────────────────────
  mp facts search "deployment"          "Describe the deploy pipeline"
  mp facts search "newark"              "What happened with Newark?"
  mp knowledge search "API rate limit"  "What are the API rate limits?"

  ─── POLICY & GOVERNANCE ─────────────────────────────────────────
  mp policy list                        "What are your rules?"
  mp policy test "DROP TABLE facts"     "Would a DROP TABLE be allowed?"
  mp policy test "SELECT * FROM facts"  "Can you run a SELECT query?"
  mp policy violations                  "Any recent policy violations?"
  mp audit                              "Show me the audit trail"

  ─── SKILLS ──────────────────────────────────────────────────────
  mp skill list                         "What skills do you have?"
                                        "Triage: robot R-142 is offline
                                         at Newark"
                                        "Run through the deploy
                                         verification checklist"

  ─── SCHEDULED JOBS ──────────────────────────────────────────────
  mp job list                           "What jobs are scheduled?"
  mp job history                        "Show me job run history"
                                        "Schedule a weekly security
                                         review for Monday at 10am"

  ─── MULTI-AGENT ─────────────────────────────────────────────────
  mp agent list                         "List all agents"
  mp agent status                       "Show agent status"
  mp send research "Compare TiKV        "Ask the research agent to
    vs CockroachDB for our use case"      compare TiKV vs CockroachDB"
  mp sync status                        "What's the sync status?"

  ─── INTERACTIVE CHAT ────────────────────────────────────────────
  mp chat                               (start chatting directly)
  mp chat research                      (chat with research agent)
  mp chat ops-bot                       (chat with ops-bot)

  ─── PORTABILITY & INTROSPECTION ─────────────────────────────────
  mp db schema                          (show full database schema)
  mp db query "SELECT pointer,
    confidence FROM facts
    WHERE status='active'
    ORDER BY confidence DESC"

  ─── GREAT DEMO QUESTIONS TO ASK ─────────────────────────────────

  "What do you know about me?" (shows fact loading works)
  "What are our top priorities for Q2?" (tests fact retrieval)
  "A robot is down at Newark — walk me through triage" (tests skills)
  "How does our deployment pipeline work?" (tests knowledge + facts)
  "Why is our pick time worse than target?" (tests cross-source search)
  "What architecture decisions did we make in Q1?" (tests retro doc)
  "Add a policy that blocks all DELETE statements" (tests policy creation)
  "Remember that we decided to use Playwright for E2E tests" (tests fact_add)
  "Schedule a daily standup summary at 9:30am" (tests job creation)
  "What happened with the TiKV migration?" (tests search across sources)
  "Compare our Q4 and Q1 metrics" (tests knowledge retrieval)
  "Who's on call and what's the escalation path?" (tests runbook retrieval)

CHEATSHEET

echo ""

if [[ "$AUTO_CHAT" -eq 1 ]]; then
  echo "  Starting interactive chat..."
  echo ""
  exec $MP chat
fi
