use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

// =========================================================================
// Types
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    Prompt,
    Tool,
    Js,
    Pipeline,
}

impl std::fmt::Display for JobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobType::Prompt => write!(f, "prompt"),
            JobType::Tool => write!(f, "tool"),
            JobType::Js => write!(f, "js"),
            JobType::Pipeline => write!(f, "pipeline"),
        }
    }
}

impl std::str::FromStr for JobType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prompt" => Ok(JobType::Prompt),
            "tool" => Ok(JobType::Tool),
            "js" => Ok(JobType::Js),
            "pipeline" => Ok(JobType::Pipeline),
            _ => anyhow::bail!("unknown job type: {s}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlapPolicy {
    Skip,
    Queue,
    Allow,
}

impl std::str::FromStr for OverlapPolicy {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "skip" => Ok(OverlapPolicy::Skip),
            "queue" => Ok(OverlapPolicy::Queue),
            "allow" => Ok(OverlapPolicy::Allow),
            _ => anyhow::bail!("unknown overlap policy: {s}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    pub description: Option<String>,
    pub schedule: String,
    pub next_run_at: i64,
    pub last_run_at: Option<i64>,
    pub job_type: String,
    pub payload: String,
    pub max_retries: i64,
    pub retry_delay_ms: i64,
    pub timeout_ms: i64,
    pub overlap_policy: String,
    pub status: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct JobRun {
    pub id: String,
    pub job_id: String,
    pub agent_id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub status: String,
    pub result: Option<String>,
    pub policy_decision: Option<String>,
    pub retry_count: i64,
    pub created_at: i64,
}

pub struct NewJob {
    pub agent_id: String,
    pub name: String,
    pub description: Option<String>,
    pub schedule: String,
    pub next_run_at: i64,
    pub job_type: String,
    pub payload: String,
    pub max_retries: Option<i64>,
    pub retry_delay_ms: Option<i64>,
    pub timeout_ms: Option<i64>,
    pub overlap_policy: Option<String>,
}

// =========================================================================
// CRUD
// =========================================================================

pub fn create_job(conn: &Connection, job: &NewJob) -> anyhow::Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO jobs (id, agent_id, name, description, schedule, next_run_at,
         job_type, payload, max_retries, retry_delay_ms, timeout_ms, overlap_policy,
         created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            id, job.agent_id, job.name, job.description, job.schedule, job.next_run_at,
            job.job_type, job.payload,
            job.max_retries.unwrap_or(0),
            job.retry_delay_ms.unwrap_or(5000),
            job.timeout_ms.unwrap_or(30000),
            job.overlap_policy.as_deref().unwrap_or("skip"),
            now, now,
        ],
    )?;
    Ok(id)
}

pub fn get_job(conn: &Connection, job_id: &str) -> anyhow::Result<Option<Job>> {
    let job = conn.query_row(
        "SELECT id, agent_id, name, description, schedule, next_run_at, last_run_at,
                job_type, payload, max_retries, retry_delay_ms, timeout_ms, overlap_policy,
                status, enabled, created_at, updated_at
         FROM jobs WHERE id = ?1",
        [job_id],
        |r| Ok(Job {
            id: r.get(0)?,
            agent_id: r.get(1)?,
            name: r.get(2)?,
            description: r.get(3)?,
            schedule: r.get(4)?,
            next_run_at: r.get(5)?,
            last_run_at: r.get(6)?,
            job_type: r.get(7)?,
            payload: r.get(8)?,
            max_retries: r.get(9)?,
            retry_delay_ms: r.get(10)?,
            timeout_ms: r.get(11)?,
            overlap_policy: r.get(12)?,
            status: r.get(13)?,
            enabled: r.get::<_, i64>(14)? != 0,
            created_at: r.get(15)?,
            updated_at: r.get(16)?,
        }),
    ).ok();
    Ok(job)
}

pub fn list_jobs(conn: &Connection, agent_id: Option<&str>) -> anyhow::Result<Vec<Job>> {
    let query = match agent_id {
        Some(_) => "SELECT id, agent_id, name, description, schedule, next_run_at, last_run_at,
                job_type, payload, max_retries, retry_delay_ms, timeout_ms, overlap_policy,
                status, enabled, created_at, updated_at
         FROM jobs WHERE agent_id = ?1 ORDER BY next_run_at ASC",
        None => "SELECT id, agent_id, name, description, schedule, next_run_at, last_run_at,
                job_type, payload, max_retries, retry_delay_ms, timeout_ms, overlap_policy,
                status, enabled, created_at, updated_at
         FROM jobs ORDER BY next_run_at ASC",
    };

    let mut stmt = conn.prepare(query)?;
    let jobs = if let Some(aid) = agent_id {
        stmt.query_map([aid], row_to_job)?.collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([], row_to_job)?.collect::<Result<Vec<_>, _>>()?
    };
    Ok(jobs)
}

fn row_to_job(r: &rusqlite::Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: r.get(0)?,
        agent_id: r.get(1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        schedule: r.get(4)?,
        next_run_at: r.get(5)?,
        last_run_at: r.get(6)?,
        job_type: r.get(7)?,
        payload: r.get(8)?,
        max_retries: r.get(9)?,
        retry_delay_ms: r.get(10)?,
        timeout_ms: r.get(11)?,
        overlap_policy: r.get(12)?,
        status: r.get(13)?,
        enabled: r.get::<_, i64>(14)? != 0,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

pub fn pause_job(conn: &Connection, job_id: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE jobs SET status = 'paused', updated_at = ?1 WHERE id = ?2",
        params![now, job_id],
    )?;
    Ok(())
}

pub fn resume_job(conn: &Connection, job_id: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE jobs SET status = 'active', updated_at = ?1 WHERE id = ?2",
        params![now, job_id],
    )?;
    Ok(())
}

pub fn delete_job(conn: &Connection, job_id: &str) -> anyhow::Result<()> {
    conn.execute("DELETE FROM jobs WHERE id = ?1", [job_id])?;
    Ok(())
}

// =========================================================================
// Job runs
// =========================================================================

pub fn start_run(conn: &Connection, job_id: &str, agent_id: &str) -> anyhow::Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO job_runs (id, job_id, agent_id, started_at, status, created_at)
         VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
        params![id, job_id, agent_id, now, now],
    )?;
    Ok(id)
}

pub fn finish_run(
    conn: &Connection,
    run_id: &str,
    status: &str,
    result: Option<&str>,
    policy_decision: Option<&str>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE job_runs SET ended_at = ?1, status = ?2, result = ?3, policy_decision = ?4
         WHERE id = ?5",
        params![now, status, result, policy_decision, run_id],
    )?;
    Ok(())
}

pub fn get_runs(conn: &Connection, job_id: &str, limit: usize) -> anyhow::Result<Vec<JobRun>> {
    let mut stmt = conn.prepare(
        "SELECT id, job_id, agent_id, started_at, ended_at, status, result,
                policy_decision, retry_count, created_at
         FROM job_runs WHERE job_id = ?1 ORDER BY rowid DESC LIMIT ?2"
    )?;
    let runs = stmt.query_map(params![job_id, limit], |r| {
        Ok(JobRun {
            id: r.get(0)?,
            job_id: r.get(1)?,
            agent_id: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            status: r.get(5)?,
            result: r.get(6)?,
            policy_decision: r.get(7)?,
            retry_count: r.get(8)?,
            created_at: r.get(9)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(runs)
}

pub fn has_running_run(conn: &Connection, job_id: &str) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM job_runs WHERE job_id = ?1 AND status = 'running'",
        [job_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

// =========================================================================
// Scheduler poll
// =========================================================================

/// Find all due jobs that should run now.
pub fn poll_due_jobs(conn: &Connection, now: i64) -> anyhow::Result<Vec<Job>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, name, description, schedule, next_run_at, last_run_at,
                job_type, payload, max_retries, retry_delay_ms, timeout_ms, overlap_policy,
                status, enabled, created_at, updated_at
         FROM jobs
         WHERE enabled = 1 AND status = 'active' AND next_run_at <= ?1
         ORDER BY next_run_at ASC"
    )?;
    let jobs = stmt.query_map([now], row_to_job)?.collect::<Result<Vec<_>, _>>()?;
    Ok(jobs)
}

/// Execute a single due job: policy check → start run → execute → finish run → update schedule.
pub fn dispatch_job(
    conn: &Connection,
    job: &Job,
    executor: &dyn Fn(&Job) -> anyhow::Result<String>,
) -> anyhow::Result<JobRun> {
    // Check overlap policy
    if job.overlap_policy == "skip" && has_running_run(conn, &job.id)? {
        let run_id = start_run(conn, &job.id, &job.agent_id)?;
        finish_run(conn, &run_id, "skipped", Some("overlap policy: skip"), None)?;
        let runs = get_runs(conn, &job.id, 1)?;
        return Ok(runs.into_iter().next().unwrap());
    }

    // Policy check
    let policy_req = crate::policy::PolicyRequest {
        actor: &job.agent_id,
        action: "execute",
        resource: &format!("job:{}", job.name),
        sql_content: None,
        channel: None,
    };
    let policy_decision = crate::policy::evaluate(conn, &policy_req)?;

    let run_id = start_run(conn, &job.id, &job.agent_id)?;

    if matches!(policy_decision.effect, crate::policy::Effect::Deny) {
        finish_run(
            conn, &run_id, "denied",
            Some(policy_decision.reason.as_deref().unwrap_or("policy denied")),
            Some("denied"),
        )?;
        update_schedule(conn, &job.id)?;
        let runs = get_runs(conn, &job.id, 1)?;
        return Ok(runs.into_iter().next().unwrap());
    }

    // Execute with retry
    let max_attempts = job.max_retries + 1;
    let mut attempt = 0;
    let mut last_error;

    loop {
        match executor(job) {
            Ok(output) => {
                finish_run(conn, &run_id, "success", Some(&output), Some("allowed"))?;
                update_schedule(conn, &job.id)?;
                let runs = get_runs(conn, &job.id, 1)?;
                return Ok(runs.into_iter().next().unwrap());
            }
            Err(e) => {
                last_error = e.to_string();
                attempt += 1;
                if attempt >= max_attempts {
                    break;
                }
            }
        }
    }

    finish_run(
        conn, &run_id, "error",
        Some(&format!("Failed after {attempt} attempts: {last_error}")),
        Some("allowed"),
    )?;
    update_schedule(conn, &job.id)?;
    let runs = get_runs(conn, &job.id, 1)?;
    Ok(runs.into_iter().next().unwrap())
}

fn update_schedule(conn: &Connection, job_id: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    // Advance next_run_at by a fixed interval (simplified; real impl parses cron)
    conn.execute(
        "UPDATE jobs SET last_run_at = ?1, next_run_at = next_run_at + 60, updated_at = ?1 WHERE id = ?2",
        params![now, job_id],
    )?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema};

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    fn allow_all(conn: &Connection) {
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow-all', 0, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();
    }

    fn sample_job() -> NewJob {
        NewJob {
            agent_id: "a".into(),
            name: "daily-digest".into(),
            description: Some("Generate daily fact digest".into()),
            schedule: "0 9 * * *".into(),
            next_run_at: 1000,
            job_type: "prompt".into(),
            payload: r#"{"message": "generate digest"}"#.into(),
            max_retries: Some(2),
            retry_delay_ms: None,
            timeout_ms: None,
            overlap_policy: None,
        }
    }

    // ========================================================================
    // CRUD
    // ========================================================================

    #[test]
    fn create_and_get_job() {
        let conn = setup();
        let id = create_job(&conn, &sample_job()).unwrap();
        let job = get_job(&conn, &id).unwrap().unwrap();
        assert_eq!(job.name, "daily-digest");
        assert_eq!(job.agent_id, "a");
        assert_eq!(job.schedule, "0 9 * * *");
        assert_eq!(job.job_type, "prompt");
        assert_eq!(job.max_retries, 2);
        assert_eq!(job.overlap_policy, "skip");
        assert_eq!(job.status, "active");
        assert!(job.enabled);
    }

    #[test]
    fn list_jobs_by_agent() {
        let conn = setup();
        create_job(&conn, &sample_job()).unwrap();
        create_job(&conn, &NewJob {
            agent_id: "b".into(),
            name: "other".into(),
            ..sample_job()
        }).unwrap();

        let all = list_jobs(&conn, None).unwrap();
        assert_eq!(all.len(), 2);

        let agent_a = list_jobs(&conn, Some("a")).unwrap();
        assert_eq!(agent_a.len(), 1);
    }

    #[test]
    fn pause_and_resume_job() {
        let conn = setup();
        let id = create_job(&conn, &sample_job()).unwrap();

        pause_job(&conn, &id).unwrap();
        assert_eq!(get_job(&conn, &id).unwrap().unwrap().status, "paused");

        resume_job(&conn, &id).unwrap();
        assert_eq!(get_job(&conn, &id).unwrap().unwrap().status, "active");
    }

    #[test]
    fn delete_job_removes_it() {
        let conn = setup();
        let id = create_job(&conn, &sample_job()).unwrap();
        delete_job(&conn, &id).unwrap();
        assert!(get_job(&conn, &id).unwrap().is_none());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let conn = setup();
        assert!(get_job(&conn, "nope").unwrap().is_none());
    }

    // ========================================================================
    // Job runs
    // ========================================================================

    #[test]
    fn start_and_finish_run() {
        let conn = setup();
        let jid = create_job(&conn, &sample_job()).unwrap();
        let rid = start_run(&conn, &jid, "a").unwrap();

        assert!(has_running_run(&conn, &jid).unwrap());

        finish_run(&conn, &rid, "success", Some("done"), Some("allowed")).unwrap();

        assert!(!has_running_run(&conn, &jid).unwrap());

        let runs = get_runs(&conn, &jid, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert_eq!(runs[0].result.as_deref(), Some("done"));
    }

    #[test]
    fn get_runs_respects_limit() {
        let conn = setup();
        let jid = create_job(&conn, &sample_job()).unwrap();
        for _ in 0..5 {
            let rid = start_run(&conn, &jid, "a").unwrap();
            finish_run(&conn, &rid, "success", None, None).unwrap();
        }
        let runs = get_runs(&conn, &jid, 3).unwrap();
        assert_eq!(runs.len(), 3);
    }

    // ========================================================================
    // Scheduler poll
    // ========================================================================

    #[test]
    fn poll_finds_due_jobs() {
        let conn = setup();
        create_job(&conn, &NewJob {
            next_run_at: 500,
            ..sample_job()
        }).unwrap();
        create_job(&conn, &NewJob {
            name: "future".into(),
            next_run_at: 9999,
            ..sample_job()
        }).unwrap();

        let due = poll_due_jobs(&conn, 1000).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].name, "daily-digest");
    }

    #[test]
    fn poll_excludes_paused_jobs() {
        let conn = setup();
        let id = create_job(&conn, &NewJob { next_run_at: 500, ..sample_job() }).unwrap();
        pause_job(&conn, &id).unwrap();

        let due = poll_due_jobs(&conn, 1000).unwrap();
        assert!(due.is_empty());
    }

    #[test]
    fn poll_excludes_disabled_jobs() {
        let conn = setup();
        let id = create_job(&conn, &NewJob { next_run_at: 500, ..sample_job() }).unwrap();
        conn.execute("UPDATE jobs SET enabled = 0 WHERE id = ?1", [&id]).unwrap();

        let due = poll_due_jobs(&conn, 1000).unwrap();
        assert!(due.is_empty());
    }

    // ========================================================================
    // Dispatch
    // ========================================================================

    #[test]
    fn dispatch_successful_job() {
        let conn = setup();
        allow_all(&conn);
        let id = create_job(&conn, &NewJob { next_run_at: 500, ..sample_job() }).unwrap();
        let job = get_job(&conn, &id).unwrap().unwrap();

        let run = dispatch_job(&conn, &job, &|_| Ok("digest complete".into())).unwrap();
        assert_eq!(run.status, "success");
        assert_eq!(run.result.as_deref(), Some("digest complete"));

        let updated = get_job(&conn, &id).unwrap().unwrap();
        assert!(updated.last_run_at.is_some());
    }

    #[test]
    fn dispatch_denied_by_policy() {
        let conn = setup();
        // No allow-all → deny-by-default
        let id = create_job(&conn, &NewJob { next_run_at: 500, ..sample_job() }).unwrap();
        let job = get_job(&conn, &id).unwrap().unwrap();

        let run = dispatch_job(&conn, &job, &|_| panic!("should not execute")).unwrap();
        assert_eq!(run.status, "denied");
        assert_eq!(run.policy_decision.as_deref(), Some("denied"));
    }

    #[test]
    fn dispatch_retries_on_failure() {
        let conn = setup();
        allow_all(&conn);
        let id = create_job(&conn, &NewJob {
            next_run_at: 500,
            max_retries: Some(2),
            ..sample_job()
        }).unwrap();
        let job = get_job(&conn, &id).unwrap().unwrap();

        let call_count = std::cell::Cell::new(0);
        let run = dispatch_job(&conn, &job, &|_| {
            let n = call_count.get();
            call_count.set(n + 1);
            if n < 2 {
                anyhow::bail!("transient error")
            } else {
                Ok("ok".into())
            }
        }).unwrap();

        assert_eq!(run.status, "success");
        assert_eq!(call_count.get(), 3); // 1 initial + 2 retries
    }

    #[test]
    fn dispatch_exhausts_retries() {
        let conn = setup();
        allow_all(&conn);
        let id = create_job(&conn, &NewJob {
            next_run_at: 500,
            max_retries: Some(1),
            ..sample_job()
        }).unwrap();
        let job = get_job(&conn, &id).unwrap().unwrap();

        let run = dispatch_job(&conn, &job, &|_| anyhow::bail!("always fails")).unwrap();
        assert_eq!(run.status, "error");
        assert!(run.result.as_deref().unwrap().contains("always fails"));
    }

    #[test]
    fn dispatch_skips_overlapping() {
        let conn = setup();
        allow_all(&conn);
        let id = create_job(&conn, &NewJob { next_run_at: 500, ..sample_job() }).unwrap();

        // Simulate a running job
        start_run(&conn, &id, "a").unwrap();

        let job = get_job(&conn, &id).unwrap().unwrap();
        let run = dispatch_job(&conn, &job, &|_| panic!("should not execute")).unwrap();
        assert_eq!(run.status, "skipped");
    }

    // ========================================================================
    // Job type parsing
    // ========================================================================

    #[test]
    fn job_type_roundtrip() {
        for jt in [JobType::Prompt, JobType::Tool, JobType::Js, JobType::Pipeline] {
            let s = jt.to_string();
            let parsed: JobType = s.parse().unwrap();
            assert_eq!(parsed, jt);
        }
    }

    #[test]
    fn job_type_invalid() {
        assert!("garbage".parse::<JobType>().is_err());
    }
}
