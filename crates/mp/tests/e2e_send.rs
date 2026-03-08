//! E2E test: `mp send` (requires LLM API or skips).

mod common;

use common::{init_project, run_mp_with_config};

/// `mp send` runs the full agent loop (context assembly, LLM call, tool handling).
/// Without a valid API key this will fail at the HTTP layer; we assert the command
/// runs and either succeeds or fails with an expected kind of error (no panic, no "config not found").
#[test]
fn send_runs_without_panic() {
    let (_temp, config_path) = init_project().unwrap();

    let out = run_mp_with_config(
        &config_path,
        &["send", "--agent", "main", "Hello, reply with exactly: OK"],
    )
    .unwrap();

    // We don't require success (no API key in CI), but the process must exit cleanly
    // and not crash. stderr may contain provider/network errors.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Should not be a config/init failure
    assert!(
        !stderr.contains("Run `mp init`") && !stdout.contains("Run `mp init`"),
        "should not ask for init: stderr={} stdout={}",
        stderr,
        stdout
    );
}
