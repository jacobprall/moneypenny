//! Stop command — shut down the gateway.

use anyhow::Result;

pub async fn run(ctx: &crate::context::CommandContext<'_>) -> Result<()> {
    let config = ctx.config;
    let pid_path = config.data_dir.join("mp.pid");
    if !pid_path.exists() {
        println!(
            "  No running gateway found (no PID file at {}).",
            pid_path.display()
        );
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str
        .trim()
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid PID in {}: {e}", pid_path.display()))?;

    println!("  Sending SIGTERM to gateway (pid {pid})...");
    #[cfg(unix)]
    {
        let status = unsafe { libc::kill(pid, libc::SIGTERM) };
        if status == 0 {
            println!("  Signal sent. Gateway should shut down gracefully.");
            let _ = std::fs::remove_file(&pid_path);
        } else {
            let errno = std::io::Error::last_os_error();
            if errno.raw_os_error() == Some(libc::ESRCH) {
                println!("  Process {pid} not found. Cleaning up stale PID file.");
                let _ = std::fs::remove_file(&pid_path);
            } else {
                anyhow::bail!("Failed to send signal to pid {pid}: {errno}");
            }
        }
    }
    #[cfg(not(unix))]
    {
        println!("  Signal-based stop is only supported on Unix. Kill process {pid} manually.");
    }
    Ok(())
}
