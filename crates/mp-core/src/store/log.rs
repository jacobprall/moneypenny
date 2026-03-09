#[path = "log/messages.rs"]
pub mod messages;
#[path = "log/policy_audit.rs"]
pub mod policy_audit;
#[path = "log/tool_calls.rs"]
pub mod tool_calls;

pub use messages::{
    Message, Session, append_message, create_session, end_session, get_messages,
    get_recent_messages, get_session, messages_without_embedding, set_message_embedding,
    set_message_embedding_with_meta, update_summary,
};
pub use policy_audit::{
    policy_audit_projection_expr, policy_audit_without_embedding, set_policy_audit_embedding,
    set_policy_audit_embedding_with_meta,
};
pub use tool_calls::{
    ToolCallRecord, get_tool_calls, record_tool_call, set_tool_call_embedding,
    set_tool_call_embedding_with_meta, tool_call_projection_expr, tool_calls_without_embedding,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema};
    use rusqlite::Connection;
    use rusqlite::params;

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    #[test]
    fn create_and_get_session() {
        let conn = setup();
        let sid = create_session(&conn, "agent-main", Some("cli")).unwrap();
        let s = get_session(&conn, &sid).unwrap().unwrap();
        assert_eq!(s.agent_id, "agent-main");
        assert_eq!(s.channel.as_deref(), Some("cli"));
        assert!(s.ended_at.is_none());
    }

    #[test]
    fn end_session_sets_ended_at() {
        let conn = setup();
        let sid = create_session(&conn, "agent-main", None).unwrap();
        end_session(&conn, &sid).unwrap();
        let s = get_session(&conn, &sid).unwrap().unwrap();
        assert!(s.ended_at.is_some());
    }

    #[test]
    fn update_session_summary() {
        let conn = setup();
        let sid = create_session(&conn, "agent-main", None).unwrap();
        update_summary(&conn, &sid, "User asked about Rust.").unwrap();
        let s = get_session(&conn, &sid).unwrap().unwrap();
        assert_eq!(s.summary.as_deref(), Some("User asked about Rust."));
    }

    #[test]
    fn append_and_get_messages() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        append_message(&conn, &sid, "user", "hello").unwrap();
        append_message(&conn, &sid, "assistant", "hi there").unwrap();

        let msgs = get_messages(&conn, &sid).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
    }

    #[test]
    fn get_recent_messages_limits_and_orders() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        for i in 0..10 {
            append_message(&conn, &sid, "user", &format!("msg {i}")).unwrap();
        }
        let recent = get_recent_messages(&conn, &sid, 3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].content, "msg 7");
        assert_eq!(recent[2].content, "msg 9");
    }

    #[test]
    fn record_and_get_tool_calls() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        let mid = append_message(&conn, &sid, "assistant", "calling tool").unwrap();

        record_tool_call(
            &conn,
            &mid,
            &sid,
            "shell_exec",
            Some(r#"{"cmd":"ls"}"#),
            Some("file.txt"),
            Some("success"),
            Some("allowed"),
            Some(42),
        )
        .unwrap();

        let calls = get_tool_calls(&conn, &sid).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_name, "shell_exec");
        assert_eq!(calls[0].status.as_deref(), Some("success"));
        assert_eq!(calls[0].duration_ms, Some(42));
    }

    #[test]
    fn messages_are_append_only_ordered() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        let m1 = append_message(&conn, &sid, "user", "first").unwrap();
        let m2 = append_message(&conn, &sid, "assistant", "second").unwrap();

        let msgs = get_messages(&conn, &sid).unwrap();
        assert_eq!(msgs[0].id, m1);
        assert_eq!(msgs[1].id, m2);
    }

    #[test]
    fn get_nonexistent_session_returns_none() {
        let conn = setup();
        assert!(get_session(&conn, "nope").unwrap().is_none());
    }

    #[test]
    fn message_embedding_roundtrip() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        let mid = append_message(&conn, &sid, "user", "embed this message").unwrap();

        let pending = messages_without_embedding(&conn, "a").unwrap();
        assert!(pending.iter().any(|(id, _)| id == &mid));

        let blob = vec![0_u8; 16];
        set_message_embedding(&conn, &mid, &blob).unwrap();

        let pending_after = messages_without_embedding(&conn, "a").unwrap();
        assert!(!pending_after.iter().any(|(id, _)| id == &mid));
    }

    #[test]
    fn tool_call_embedding_roundtrip() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        let mid = append_message(&conn, &sid, "assistant", "running tool").unwrap();
        let tcid = record_tool_call(
            &conn,
            &mid,
            &sid,
            "shell_exec",
            Some(r#"{"command":"ls"}"#),
            Some("ok"),
            Some("success"),
            Some("allow"),
            Some(5),
        )
        .unwrap();

        let pending = tool_calls_without_embedding(&conn, "a").unwrap();
        assert!(pending.iter().any(|(id, _)| id == &tcid));

        set_tool_call_embedding(&conn, &tcid, &[0_u8; 16]).unwrap();
        let pending_after = tool_calls_without_embedding(&conn, "a").unwrap();
        assert!(!pending_after.iter().any(|(id, _)| id == &tcid));
    }

    #[test]
    fn policy_audit_embedding_roundtrip() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        conn.execute(
            "INSERT INTO policy_audit (id, policy_id, actor, action, resource, effect, reason, session_id, created_at)
             VALUES ('pa-1', 'p1', 'a', 'call', 'shell_exec', 'deny', 'blocked', ?1, 1)",
            params![sid],
        ).unwrap();

        let pending = policy_audit_without_embedding(&conn, "a").unwrap();
        assert!(pending.iter().any(|(id, _)| id == "pa-1"));

        set_policy_audit_embedding(&conn, "pa-1", &[0_u8; 16]).unwrap();
        let pending_after = policy_audit_without_embedding(&conn, "a").unwrap();
        assert!(!pending_after.iter().any(|(id, _)| id == "pa-1"));
    }
}
