# Q1 2026 Retrospective — Acme Robotics

## Summary

Q1 was a mixed quarter. We hit our reliability targets for the first time but
missed on pick time optimization and hiring. The CockroachDB-to-TiKV migration
is progressing well and should complete in Q2.

## What Went Well

### Newark Facility Launch
- Brought Newark online on Feb 3rd, two weeks ahead of schedule
- Zero SEV1 incidents during the first 30 days
- Fleet expanded from 130 to 200 robots across three sites
- Newark running at 97.2% uptime (above 95% ramp target)

### Reliability Improvements
- Orchestrator rewrite to Rust completed — P99 latency dropped from 180ms to 12ms
- New watchdog system catches stuck robots within 30 seconds (was 5 minutes)
- Firmware v3.1 reduced LiDAR false positives by 60%
- Achieved 99.7% fulfillment rate (best quarter ever)

### SOC 2 Progress
- Completed readiness assessment with external auditor
- 14 of 18 controls fully implemented
- Remaining 4 controls (access review automation, DR testing, vendor assessment,
  log retention policy) are in progress for Q2

## What Didn't Go Well

### Pick Time Regression
- Mean pick time increased from 3.8s to 4.2s after Newark launch
- Root cause: Newark shelf spacing is 15% narrower than Austin/Chicago
- Inverse kinematics solver generates conservative trajectories for tight spaces
- Fix requires retraining the trajectory model on Newark-specific geometry

### Hiring
- Target: 3 senior perception engineers. Hired: 0.
- Lost two final-round candidates to Waymo and Figure
- Adjusted comp bands in March — two offers now outstanding for April start

### Database Migration Stalled
- TiKV migration blocked for 3 weeks by a replication bug (TiKV issue #14892)
- Contributed a fix upstream — merged March 15
- Migration now at 40% (read path complete, write path in progress)
- Revised completion estimate: end of May (was end of March)

## Key Decisions Made

1. **Rust for new services** — All new services will be written in Rust.
   Go services will be migrated opportunistically. Rationale: Orchestrator
   rewrite proved 15x latency improvement with better memory safety.

2. **Edge-first vision processing** — All camera data stays on-device.
   No cloud vision APIs. This simplifies PII compliance and reduces latency.

3. **Abandon multi-region Orchestrator** — Instead of running Orchestrator
   in each region, we'll use a single-region primary with fast failover.
   Simpler architecture, same effective availability.

4. **Weekly architecture review** — Every Tuesday at 10am. Mandatory for
   team leads, optional for ICs. Decisions documented in `#arch-decisions`.

## Q2 Goals

1. Reduce mean pick time to 3.5s (retrain trajectory model for Newark geometry)
2. Complete SOC 2 Type II audit
3. Finish TiKV migration (target: end of May)
4. Hire 3 senior perception engineers
5. Ship firmware v3.2 to all sites
6. Implement automated canary analysis (replace manual canary approval)
7. Reduce MTTR from 12 minutes to 10 minutes

## Metrics

| Metric | Q4 2025 | Q1 2026 | Target |
|--------|---------|---------|--------|
| Fulfillment rate | 99.4% | 99.7% | 99.9% |
| Mean pick time | 3.8s | 4.2s | 3.5s |
| Robot uptime | 97.5% | 98.1% | 99.5% |
| Incident MTTR | 15min | 12min | 10min |
| Fleet size | 130 | 200 | 200 |
| SEV1 incidents | 3 | 1 | 0 |
