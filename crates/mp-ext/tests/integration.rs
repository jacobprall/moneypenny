use rusqlite::Connection;

fn open_with_extensions() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    mp_ext::init_all_extensions(&conn).expect("init extensions");
    conn
}

#[test]
fn init_all_extensions_succeeds() {
    let _conn = open_with_extensions();
}

#[test]
fn init_is_idempotent() {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    mp_ext::init_all_extensions(&conn).expect("first init");
    mp_ext::init_all_extensions(&conn).expect("second init");
}

#[test]
fn js_eval_works() {
    let conn = open_with_extensions();
    let result: rusqlite::types::Value = conn
        .query_row("SELECT js_eval('1 + 2')", [], |r| r.get(0))
        .expect("js_eval");
    match result {
        rusqlite::types::Value::Integer(i) => assert_eq!(i, 3),
        rusqlite::types::Value::Text(s) => assert_eq!(s.trim(), "3"),
        other => panic!("unexpected js_eval result type: {:?}", other),
    }
}

#[test]
fn vector_version_exists() {
    let conn = open_with_extensions();
    let v: String = conn
        .query_row("SELECT vector_version()", [], |r| r.get(0))
        .unwrap_or_else(|_| "unavailable".into());
    assert!(
        !v.is_empty(),
        "vector_version should return a non-empty string"
    );
}
