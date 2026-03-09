//! Shared helpers for integration and e2e tests.
//!
//! Run the `mp` binary in a temp directory with a fresh config so tests are isolated.

use std::path::Path;
use std::process::{Child, Command, Output};

/// Path to the `mp` binary (set by Cargo when building the package with [[bin]]).
const MP_BIN: &str = env!("CARGO_BIN_EXE_mp");

/// Config filename used in the temp dir.
pub const CONFIG_NAME: &str = "moneypenny.toml";

/// Run `mp` with the given args, optionally from a specific working directory.
///
/// If `cwd` is `Some`, the process runs with that current directory (so relative
/// paths in config like `./mp-data` resolve there). Stderr is captured so test
/// output is readable; use `output.stderr` to assert on errors.
pub fn run_mp<I, S>(args: I, cwd: Option<&Path>) -> std::io::Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut cmd = Command::new(MP_BIN);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.env_remove("RUST_LOG"); // avoid log noise
    cmd.output()
}

/// Create a temp directory, run `mp init --config moneypenny.toml` from that dir,
/// then return the temp dir guard and the path to the config file.
///
/// Using a relative config path and cwd=temp_dir ensures default `data_dir = "./mp-data"`
/// resolves to `<temp_dir>/mp-data`, so the agent DB is at `<temp_dir>/mp-data/main.db`.
pub fn init_project() -> Result<(tempfile::TempDir, std::path::PathBuf), Box<dyn std::error::Error>>
{
    let temp = tempfile::tempdir()?;
    let config_path = temp.path().join(CONFIG_NAME);

    let out = run_mp(["--config", CONFIG_NAME, "init"], Some(temp.path()))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        panic!(
            "mp init failed: status={}\nstdout:\n{}\nstderr:\n{}",
            out.status, stdout, stderr
        );
    }

    assert!(config_path.exists(), "config file should exist after init");
    Ok((temp, config_path))
}

/// Run `mp` with `--config <path>` from the directory that contains the config.
/// Use this for all commands after init (so that `data_dir` relative paths resolve).
pub fn run_mp_with_config(config_path: &Path, args: &[&str]) -> std::io::Result<Output> {
    let cwd = config_path.parent().unwrap_or(Path::new("."));
    let mut all = vec!["--config", config_path.to_str().unwrap()];
    all.extend(args.iter().copied());
    run_mp(all, Some(cwd))
}

/// Patch the config file to enable the HTTP channel and disable CLI.
/// Use before spawning the gateway so `mp start` runs the server without waiting for stdin.
pub fn enable_http_channel(
    config_path: &Path,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let s = std::fs::read_to_string(config_path)?;
    let mut t: toml::Table = toml::from_str(&s)?;
    let channels = t
        .entry("channels")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or("channels is not a table")?;
    channels.insert("cli".into(), toml::Value::Boolean(false));
    let http = toml::Table::from_iter([("port".to_string(), toml::Value::Integer(port as i64))]);
    channels.insert("http".into(), toml::Value::Table(http));
    std::fs::write(config_path, toml::to_string_pretty(&t)?)?;
    Ok(())
}

/// Spawn `mp start` in the background. Config must already have HTTP channel enabled
/// and CLI disabled (e.g. via `enable_http_channel`). Returns the child process handle;
/// caller must kill it when done (e.g. `child.kill()`).
pub fn spawn_gateway(config_path: &Path) -> std::io::Result<Child> {
    let cwd = config_path.parent().unwrap_or(Path::new("."));
    let mut cmd = Command::new(MP_BIN);
    cmd.args(["--config", config_path.to_str().unwrap(), "start"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.env_remove("RUST_LOG");
    cmd.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp_bin_exists() {
        assert!(
            std::path::Path::new(MP_BIN).exists(),
            "binary should exist when tests run"
        );
    }
}
