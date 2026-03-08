# Acme Robotics — Project Handbook

## Mission

Acme Robotics builds autonomous warehouse robots that can navigate, pick, and pack
orders without human intervention. Our fleet of 200 robots operates across three
fulfillment centers in Austin, Chicago, and Newark.

## Architecture

The system is built on a microservices architecture with four core services:

- **Navigator** — path planning and obstacle avoidance using LiDAR point clouds
- **Picker** — robotic arm control with 6-DOF inverse kinematics
- **Orchestrator** — fleet coordination, task assignment, and load balancing
- **Vision** — real-time object detection using YOLOv8 running on edge TPUs

All services communicate over gRPC with Protocol Buffers. State is stored in
CockroachDB for distributed consistency. Event sourcing through Kafka provides
an audit trail of every robot action.

## Deployment

Production deploys happen every Tuesday and Thursday via ArgoCD. The deployment
pipeline runs: lint → unit tests → integration tests → canary (5% traffic for
30 minutes) → full rollout. Rollbacks are automatic if error rate exceeds 0.1%.

Emergency hotfixes can bypass the canary stage with VP-level approval and a
post-incident review within 48 hours.

## Security Policy

- All inter-service communication uses mTLS with certificates rotated every 90 days
- Robot firmware updates require dual-signature verification (engineering + security)
- PII from warehouse cameras is processed on-device and never leaves the edge node
- Access to production databases requires MFA and is logged to Splunk
- Penetration testing is conducted quarterly by an external firm

## Team Structure

- **Platform team** (8 engineers) — owns Navigator and Orchestrator
- **Manipulation team** (5 engineers) — owns Picker and gripper hardware
- **Perception team** (6 engineers) — owns Vision and sensor fusion
- **SRE team** (4 engineers) — owns infrastructure, monitoring, incident response

On-call rotation is weekly, shared across all teams. Escalation path:
on-call → team lead → VP Engineering → CTO.

## Key Metrics

- **Order fulfillment rate**: 99.7% (target: 99.9%)
- **Mean pick time**: 4.2 seconds (target: 3.5 seconds)
- **Robot uptime**: 98.1% (target: 99.5%)
- **Incident MTTR**: 12 minutes (target: 10 minutes)

## Current Priorities

1. Reduce mean pick time from 4.2s to 3.5s by optimizing arm trajectory planning
2. Roll out firmware v3.2 with improved LiDAR processing to all three sites
3. Complete SOC 2 Type II audit by end of Q2
4. Hire 3 senior engineers for the perception team
5. Migrate from CockroachDB to TiKV for 40% cost reduction
