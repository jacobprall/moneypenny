CREATE VIEW v_session_cost AS
SELECT session_id,
       COALESCE(SUM(cost_usd), 0)   AS total_cost_usd,
       COALESCE(SUM(tokens_in), 0)  AS total_tokens_in,
       COALESCE(SUM(tokens_out), 0) AS total_tokens_out,
       COUNT(*)                     AS run_count
FROM runs
GROUP BY session_id;

CREATE VIEW v_cost_today AS
SELECT COALESCE(SUM(cost_usd), 0) AS total,
       COUNT(DISTINCT session_id) AS sessions,
       COALESCE(SUM(tokens_in), 0) AS tokens_in,
       COALESCE(SUM(tokens_out), 0) AS tokens_out
FROM runs
WHERE date(started_at, 'unixepoch') = date('now');

CREATE VIEW v_health AS
SELECT json_object(
  'sessions_total',    (SELECT COUNT(*) FROM sessions),
  'sessions_active',   (SELECT COUNT(*) FROM sessions WHERE status IN ('active','running','paused')),
  'sessions_running',  (SELECT COUNT(*) FROM sessions WHERE status = 'running'),
  'runs_total',        (SELECT COUNT(*) FROM runs),
  'messages_total',    (SELECT COUNT(*) FROM messages),
  'chunks_total',      (SELECT COUNT(*) FROM code_chunks),
  'work_pending',      (SELECT COUNT(*) FROM work_queue WHERE processed_at IS NULL),
  'work_failed',       (SELECT COUNT(*) FROM work_queue WHERE error IS NOT NULL)
) AS health;
