use anyhow::Result;
use mp_core::config::Config;
use std::path::Path;
use std::sync::Arc;

use crate::helpers::{
    build_embedding_provider, build_provider, embed_pending, embedding_model_id, extract_facts,
    maybe_summarize_session, open_agent_db, resolve_agent,
};

// =========================================================================
// Worker subprocess and inter-worker routing bus
// =========================================================================

pub struct WorkerHandle {
    pub pid: u32,
    pub agent_name: String,
    child: tokio::process::Child,
}

impl WorkerHandle {
    pub async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
        tracing::info!(agent = %self.agent_name, pid = self.pid, "worker stopped");
    }
}

struct WorkerChannel {
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
}

pub struct WorkerBus {
    channels: tokio::sync::Mutex<std::collections::HashMap<String, WorkerChannel>>,
}

impl WorkerBus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            channels: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    pub async fn register(
        &self,
        agent_name: String,
        stdin: tokio::process::ChildStdin,
        stdout: tokio::process::ChildStdout,
    ) {
        let mut ch = self.channels.lock().await;
        ch.insert(
            agent_name,
            WorkerChannel {
                stdin,
                stdout: tokio::io::BufReader::new(stdout),
            },
        );
    }

    pub async fn route(
        &self,
        target: &str,
        message: &str,
        session_id: Option<&str>,
    ) -> anyhow::Result<String> {
        let (response, _) = self.route_full(target, message, session_id).await?;
        Ok(response)
    }

    pub async fn route_full(
        &self,
        target: &str,
        message: &str,
        session_id: Option<&str>,
    ) -> anyhow::Result<(String, String)> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let mut channels = self.channels.lock().await;
        let ch = channels
            .get_mut(target)
            .ok_or_else(|| anyhow::anyhow!("No running worker for agent '{target}'"))?;

        let req = serde_json::json!({"message": message, "session_id": session_id});
        ch.stdin.write_all(format!("{req}\n").as_bytes()).await?;
        ch.stdin.flush().await?;

        let mut line = String::new();
        ch.stdout.read_line(&mut line).await?;

        let resp: serde_json::Value = serde_json::from_str(line.trim())
            .map_err(|e| anyhow::anyhow!("worker response parse error: {e}"))?;
        if let Some(err) = resp["error"].as_str() {
            anyhow::bail!("worker reported error: {err}");
        }
        let response = resp["response"].as_str().unwrap_or("").to_string();
        let sid = resp["session_id"].as_str().unwrap_or("").to_string();
        Ok((response, sid))
    }
}

pub fn spawn_worker(
    _config: &Config,
    config_path: &Path,
    agent_name: &str,
) -> Result<(
    WorkerHandle,
    tokio::process::ChildStdin,
    tokio::process::ChildStdout,
)> {
    let exe = std::env::current_exe()?;
    let config_abs = if config_path.is_absolute() {
        config_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(config_path)
    };
    let config_dir = config_abs.parent().unwrap_or_else(|| Path::new("."));
    let mut child = tokio::process::Command::new(&exe)
        .current_dir(config_dir)
        .arg("--config")
        .arg(&config_abs)
        .arg("worker")
        .arg("--agent")
        .arg(agent_name)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    let pid = child.id().unwrap_or(0);
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("worker process has no stdin pipe"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("worker process has no stdout pipe"))?;

    Ok((
        WorkerHandle {
            pid,
            agent_name: agent_name.to_string(),
            child,
        },
        stdin,
        stdout,
    ))
}

pub async fn cmd_worker(config: &Config, agent_name: &str) -> Result<()> {
    let agent = resolve_agent(config, Some(agent_name))?;
    let conn = open_agent_db(config, &agent.name)?;
    let provider = build_provider(agent)?;
    let embed = build_embedding_provider(config, agent).ok();

    tracing::info!(agent = agent_name, "worker started");

    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({"error": e.to_string()});
                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes())
                    .await?;
                continue;
            }
        };

        let msg = request["message"].as_str().unwrap_or("");
        let session_id = request["session_id"].as_str();

        let sid = if let Some(s) = session_id {
            s.to_string()
        } else {
            mp_core::store::log::create_session(&conn, &agent.name, Some("gateway"))?
        };

        let response = match crate::agent::agent_turn(
            &conn,
            provider.as_ref(),
            embed.as_deref(),
            &agent.name,
            &sid,
            agent.persona.as_deref(),
            msg,
            agent.policy_mode(),
            None,
        )
        .await
        {
            Ok(r) => serde_json::json!({"response": r, "session_id": sid}),
            Err(e) => serde_json::json!({"error": e.to_string(), "session_id": sid}),
        };

        tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{response}\n").as_bytes())
            .await?;
        tokio::io::AsyncWriteExt::flush(&mut stdout).await?;

        let _ = extract_facts(&conn, provider.as_ref(), &agent.name, &sid).await;
        if let Some(ref ep) = embed {
            let model_id = embedding_model_id(agent);
            embed_pending(&conn, ep.as_ref(), &agent.name, &model_id).await;
        }
        maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
    }

    tracing::info!(agent = agent_name, "worker exiting");
    Ok(())
}

// =========================================================================
// Scheduler
// =========================================================================

pub async fn run_scheduler(config: &Config, shutdown: &mut tokio::sync::broadcast::Receiver<()>) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            _ = shutdown.recv() => {
                tracing::info!("scheduler shutting down");
                return;
            }
        }

        for agent in &config.agents {
            let conn = match open_agent_db(config, &agent.name) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(agent = %agent.name, error = %e, "scheduler: failed to open db");
                    continue;
                }
            };

            let now = chrono::Utc::now().timestamp();
            let due_jobs = match mp_core::scheduler::poll_due_jobs(&conn, &agent.name, now) {
                Ok(jobs) => jobs,
                Err(e) => {
                    tracing::warn!(agent = %agent.name, error = %e, "scheduler: poll failed");
                    continue;
                }
            };

            for job in &due_jobs {
                tracing::info!(agent = %agent.name, job = %job.name, "scheduler: dispatching");
                let result = mp_core::scheduler::dispatch_job(&conn, job, &|j| {
                    mp_core::scheduler::execute_job_payload(&conn, j)
                });
                match result {
                    Ok(run) => {
                        tracing::info!(
                            agent = %agent.name, job = %job.name,
                            status = %run.status, "scheduler: job completed"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            agent = %agent.name, job = %job.name,
                            error = %e, "scheduler: dispatch failed"
                        );
                    }
                }
            }
        }
    }
}
