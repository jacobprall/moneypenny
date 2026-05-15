# Computed Intelligence Views

### Problem

The agent and UI have no way to get a quick health check without ad-hoc
queries. SQL views compute health metrics and actionable suggestions.

### `mp_health` view

```sql
CREATE VIEW mp_health AS
SELECT
  (SELECT COUNT(*) FROM sessions) AS total_sessions,
  (SELECT COUNT(*) FROM sessions
   WHERE last_activity_at > unixepoch() - 86400) AS sessions_today,
  (SELECT COALESCE(SUM(json_extract(cost, '$.totalCost')), 0) FROM sessions
   WHERE last_activity_at > unixepoch() - 86400) AS cost_today_usd,
  (SELECT COALESCE(SUM(json_extract(cost, '$.totalCost')), 0) FROM sessions
   WHERE last_activity_at > unixepoch() - 604800) AS cost_week_usd,
  (SELECT COUNT(*) FROM messages WHERE role = 'assistant') AS total_responses,
  (SELECT COUNT(*) FROM skills) AS total_skills,
  (SELECT COUNT(*) FROM knowledge) AS total_knowledge_entries,
  (SELECT COUNT(*) FROM jobs WHERE enabled = 1) AS active_jobs,
  (SELECT COUNT(*) FROM job_runs
   WHERE status = 'failed' AND created_at > unixepoch() - 86400) AS failed_jobs_today,
  (SELECT COUNT(*) FROM gov_events
   WHERE created_at > unixepoch() - 86400) AS gov_events_today,
  (SELECT COUNT(*) FROM gov_events
   WHERE effect = 'deny' AND created_at > unixepoch() - 86400) AS denied_today,
  (SELECT MAX(last_activity_at) FROM sessions) AS last_session_at,
  (SELECT MAX(created_at) FROM job_runs) AS last_job_run_at,
  (SELECT COUNT(*) FROM sessions WHERE archived_at IS NOT NULL) AS archived_sessions,
  (SELECT COUNT(*) FROM sessions WHERE archived_at IS NULL
   AND id IN (SELECT session_id FROM compaction_markers)) AS compacted_sessions;
```

### `mp_suggestions` view

```sql
CREATE VIEW mp_suggestions AS
-- Stale sessions that could be compacted
SELECT
  'compact_session' AS suggestion_type,
  s.id AS target_id,
  s.label AS target_label,
  'Session has ' || mc.msg_count || ' messages and no compaction marker' AS reason,
  mc.msg_count AS priority_score
FROM sessions s
JOIN (SELECT session_id, COUNT(*) AS msg_count FROM messages GROUP BY session_id) mc
  ON mc.session_id = s.id
LEFT JOIN compaction_markers cm ON cm.session_id = s.id
WHERE mc.msg_count > 50 AND cm.id IS NULL AND s.archived_at IS NULL

UNION ALL

-- Compacted sessions eligible for archival
SELECT
  'archive_session' AS suggestion_type,
  s.id AS target_id,
  s.label AS target_label,
  'Session compacted ' || CAST((unixepoch() - cm.created_at) / 86400 AS INTEGER) || ' days ago, eligible for archival' AS reason,
  CAST((unixepoch() - cm.created_at) / 86400 AS INTEGER) AS priority_score
FROM sessions s
JOIN compaction_markers cm ON cm.session_id = s.id
WHERE s.archived_at IS NULL
  AND cm.created_at < unixepoch() - 2592000  -- 30 days

UNION ALL

-- Unused skills: not mentioned by name in any message in 30 days
SELECT
  'review_skill' AS suggestion_type,
  sk.name AS target_id,
  sk.name AS target_label,
  'Skill "' || sk.name || '" not mentioned in any message for 30+ days' AS reason,
  30 AS priority_score
FROM skills sk
WHERE NOT EXISTS (
  SELECT 1 FROM messages m
  WHERE m.created_at > unixepoch() - 2592000
    AND m.content LIKE '%' || sk.name || '%'
)

UNION ALL

-- High-cost agents (daily spend > 2x weekly average)
SELECT
  'review_cost' AS suggestion_type,
  costs.agent_name AS target_id,
  costs.agent_name AS target_label,
  'Agent spent $' || ROUND(costs.daily_cost, 4) || ' today (>' || ROUND(costs.avg_cost * 2, 4) || ' 2x avg)' AS reason,
  CAST(costs.daily_cost * 1000 AS INTEGER) AS priority_score
FROM (
  SELECT
    s.agent_name,
    SUM(CASE WHEN s.last_activity_at > unixepoch() - 86400
        THEN COALESCE(json_extract(s.cost, '$.totalCost'), 0) ELSE 0 END) AS daily_cost,
    AVG(COALESCE(json_extract(s.cost, '$.totalCost'), 0)) AS avg_cost
  FROM sessions s
  WHERE s.last_activity_at > unixepoch() - 604800
  GROUP BY s.agent_name
) costs
WHERE costs.daily_cost > costs.avg_cost * 2 AND costs.daily_cost > 0.01

UNION ALL

-- Failed jobs needing attention
SELECT
  'fix_job' AS suggestion_type,
  j.id AS target_id,
  j.name AS target_label,
  'Job failed ' || fr.fail_count || ' times in last 24h' AS reason,
  fr.fail_count * 10 AS priority_score
FROM jobs j
JOIN (
  SELECT job_id, COUNT(*) AS fail_count
  FROM job_runs
  WHERE status = 'failed' AND created_at > unixepoch() - 86400
  GROUP BY job_id
) fr ON fr.job_id = j.id
WHERE fr.fail_count >= 2

ORDER BY priority_score DESC;
```

### Acceptance criteria

- [ ] `SELECT * FROM mp_health` returns all metrics correctly, including archived/compacted counts
- [ ] `SELECT * FROM mp_suggestions` returns actionable suggestions including archival candidates
- [ ] Skill usage detection works with simple `LIKE` matching
- [ ] Cost anomaly detection correctly identifies 2x daily spikes
- [ ] `GET /api/v1/observe/health` returns mp_health data
- [ ] `GET /api/v1/observe/suggestions` returns mp_suggestions data
- [ ] `mp status` CLI command displays health + top 5 suggestions

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | `mp_health` view + API endpoint | 1 day |
| 4.2 | `mp_suggestions` view (with archival suggestions) | 1.5 days |
| 4.3 | Web UI: health dashboard + suggestion list in Observe page | 1.5 days |
| 4.4 | `mp status` CLI command | 0.5 days |
