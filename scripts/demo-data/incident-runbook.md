# Incident Response Runbook

## Severity Levels

| Level | Criteria | Response Time | Example |
|-------|----------|---------------|---------|
| SEV1 | Fleet-wide outage, >10% robots offline | 5 minutes | Orchestrator crash, network partition |
| SEV2 | Single-site degradation, SLA breach | 15 minutes | Newark facility LiDAR interference |
| SEV3 | Single robot offline, no SLA impact | 1 hour | R-142 motor fault |
| SEV4 | Cosmetic / non-urgent | Next business day | Dashboard rendering issue |

## Escalation Path

1. **On-call engineer** — first responder, available 24/7
2. **Team lead** — escalate if not resolved within response time
3. **VP Engineering** — escalate for SEV1 or customer-facing impact
4. **CTO** — escalate for data loss, security breach, or regulatory impact

## SEV1 Procedure

1. Page the on-call engineer via PagerDuty
2. Open a dedicated Slack channel: `#inc-YYYY-MM-DD-brief-description`
3. Assign an Incident Commander (IC) — usually the on-call team lead
4. IC posts initial assessment within 10 minutes
5. IC coordinates response, delegates investigation tasks
6. Status updates every 15 minutes in the incident channel
7. When resolved: IC posts root cause summary and marks incident resolved
8. Post-incident review (PIR) meeting within 48 hours
9. PIR action items tracked in Linear with `incident` label

## SEV2 Procedure

1. Alert on-call engineer via PagerDuty
2. Post in `#ops-incidents` with site name, symptom, and suspected scope
3. Investigate using Grafana dashboards (link: grafana.internal/d/fleet-health)
4. If not resolved within SLA window, escalate to team lead
5. Post resolution summary in `#ops-incidents`
6. PIR optional — team lead decides

## Common Failure Modes

### Robot Won't Move
1. Check Navigator service logs: `kubectl logs -l app=navigator -n production`
2. Verify LiDAR feed: `mp-diag lidar-check --robot <ID>`
3. Check for firmware mismatch: `mp-diag firmware-version --robot <ID>`
4. If path planning timeout: restart Navigator pod for that zone

### Arm Trajectory Failure
1. Check Picker service health: `kubectl logs -l app=picker -n production`
2. Verify inverse kinematics solver state
3. Common cause: collision detection false positive — check sensor calibration
4. If persistent: pull robot from rotation and file hardware ticket

### Fleet Coordination Lost
1. This is a SEV1 — page immediately
2. Check Orchestrator pod status across all zones
3. Verify CockroachDB cluster health
4. Check for Kafka consumer lag (> 5 minutes is critical)
5. If DB partition: do NOT attempt manual recovery. Call database team lead.

### Vision System Degraded
1. Check edge TPU temperature (throttles above 85°C)
2. Verify YOLO model version matches expected (v8.2.1)
3. Check camera feed connectivity
4. If warehouse lighting changed: recalibrate (takes ~20 minutes per zone)

## Post-Incident Review Template

```
## Incident: [Title]
Date: [YYYY-MM-DD]
Duration: [Xh Ym]
Severity: [SEV1-4]
IC: [Name]

### Timeline
- HH:MM — First alert
- HH:MM — IC assigned
- HH:MM — Root cause identified
- HH:MM — Fix deployed
- HH:MM — All-clear

### Root Cause
[What actually broke and why]

### Impact
[Users affected, orders delayed, SLA breach Y/N]

### Action Items
- [ ] [Action] — Owner: [Name] — Due: [Date]
```
