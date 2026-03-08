//! Integration tests: CLI commands after `mp init` (read-only and status commands).

mod common;

use common::{init_project, run_mp_with_config};

#[test]
fn health_succeeds_after_init() {
    let (_temp, config_path) = init_project().unwrap();

    let out = run_mp_with_config(&config_path, &["health"]).unwrap();
    assert!(out.status.success(), "mp health should succeed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Moneypenny") || stdout.contains("Gateway") || stdout.contains("Agent"), "health output: {}", stdout);
}

#[test]
fn facts_list_succeeds_after_init() {
    let (_temp, config_path) = init_project().unwrap();

    let out = run_mp_with_config(&config_path, &["facts", "list"]).unwrap();
    assert!(out.status.success(), "mp facts list should succeed: {}", String::from_utf8_lossy(&out.stderr));
    // Empty list or header
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fact") || stdout.is_empty() || stdout.trim().is_empty());
}

#[test]
fn sync_status_succeeds_after_init() {
    let (_temp, config_path) = init_project().unwrap();

    let out = run_mp_with_config(&config_path, &["sync", "status"]).unwrap();
    assert!(out.status.success(), "mp sync status should succeed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Sync") || stdout.contains("status") || stdout.contains("Site") || stdout.contains("main"));
}

#[test]
fn db_schema_succeeds_after_init() {
    let (_temp, config_path) = init_project().unwrap();

    let out = run_mp_with_config(&config_path, &["db", "schema"]).unwrap();
    assert!(out.status.success(), "mp db schema should succeed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("CREATE TABLE") || stdout.contains("facts") || stdout.contains("sqlite_master"));
}

#[test]
fn agent_status_succeeds_after_init() {
    let (_temp, config_path) = init_project().unwrap();

    let out = run_mp_with_config(&config_path, &["agent", "status"]).unwrap();
    assert!(out.status.success(), "mp agent status should succeed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("main") || stdout.contains("agent") || stdout.contains("Agent"));
}

#[test]
fn policy_list_succeeds_after_init() {
    let (_temp, config_path) = init_project().unwrap();

    let out = run_mp_with_config(&config_path, &["policy", "list"]).unwrap();
    assert!(out.status.success(), "mp policy list should succeed: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn job_list_succeeds_after_init() {
    let (_temp, config_path) = init_project().unwrap();

    let out = run_mp_with_config(&config_path, &["job", "list"]).unwrap();
    assert!(out.status.success(), "mp job list should succeed: {}", String::from_utf8_lossy(&out.stderr));
}
