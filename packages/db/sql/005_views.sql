-- Views: context assembly, health, metrics

-- The main context view: everything the agent needs in one query
CREATE VIEW IF NOT EXISTS v_agent_context AS
SELECT json_object(
    'previous_sessions', (
        SELECT json_group_array(json_object(
            'date', date(sp.created_at, 'unixepoch'),
            'key', sp.key,
            'phrase', sp.phrase,
            'pinned', sp.pinned
        ) ORDER BY sp.pinned DESC, sp.created_at DESC)
        FROM session_pointers sp
        WHERE sp.archived = 0
        LIMIT 20
    ),
    'skills', (
        SELECT json_group_array(json_object(
            'name', s.name,
            'description', s.description
        ) ORDER BY s.confidence DESC)
        FROM skills s
        WHERE s.confidence > 0.3
        LIMIT 10
    ),
    'conventions', (
        SELECT json_group_array(json_object(
            'name', c.name,
            'description', c.description
        ))
        FROM conventions c
        WHERE c.confidence > 0.5
    ),
    'policies', (
        SELECT json_group_array(json_object(
            'name', p.name,
            'effect', p.effect,
            'description', p.description
        ))
        FROM policies p
        WHERE p.enabled = 1
    ),
    'pending_work', (
        SELECT COUNT(*) FROM work_queue WHERE processed_at IS NULL
    )
) AS context;

-- Session pointer list (for the system prompt)
CREATE VIEW IF NOT EXISTS v_session_pointers AS
SELECT
    sp.*,
    s.agent_name,
    s.last_active_at,
    (SELECT COUNT(*) FROM messages WHERE session_id = sp.session_id) AS turn_count
FROM session_pointers sp
JOIN sessions s ON s.id = sp.session_id
WHERE sp.archived = 0
ORDER BY sp.pinned DESC, sp.created_at DESC;

-- Health dashboard
CREATE VIEW IF NOT EXISTS v_health AS
SELECT json_object(
    'total_sessions', (SELECT COUNT(*) FROM sessions),
    'active_sessions', (SELECT COUNT(*) FROM sessions WHERE is_active = 1),
    'total_messages', (SELECT COUNT(*) FROM messages),
    'total_pointers', (SELECT COUNT(*) FROM session_pointers WHERE archived = 0),
    'pinned_pointers', (SELECT COUNT(*) FROM session_pointers WHERE pinned = 1),
    'total_chunks', (SELECT COUNT(*) FROM code_chunks),
    'total_skills', (SELECT COUNT(*) FROM skills),
    'pending_work', (SELECT COUNT(*) FROM work_queue WHERE processed_at IS NULL),
    'failed_work', (SELECT COUNT(*) FROM work_queue WHERE error IS NOT NULL)
) AS health;
