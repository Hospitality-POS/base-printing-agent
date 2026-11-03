use crate::commands::PrintJob;
use crate::printer::{self, PrintTarget};
use crate::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a printed job is remembered, so a later poll or push of the same
/// job (e.g. while its ack is still in flight) never prints it again.
const REMEMBER_PRINTED: Duration = Duration::from_secs(60 * 60);
/// Print attempts before a job is reported as failed to the API. Failed
/// attempts leave the job pending, so the polling loop (every 10 s) retries
/// it — about five minutes to fix paper / power / cable.
const MAX_PRINT_ATTEMPTS: u32 = 30;
const ACK_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Default)]
struct JobRecord {
    printing: bool,
    printed: bool,
    failures: u32,
    updated: Option<Instant>,
}

/// Per-job print state shared by the WebSocket and polling paths.
#[derive(Default)]
pub struct JobLedger {
    jobs: Mutex<HashMap<String, JobRecord>>,
}

#[derive(Debug, PartialEq)]
enum Claim {
    /// Not printed yet and nobody is printing it: go ahead.
    Print,
    /// Already printed (its ack may have been lost): re-send the ack only.
    AlreadyPrinted,
    /// Another task is printing it right now.
    Busy,
}

impl JobLedger {
    fn claim(&self, job_id: &str) -> Claim {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        jobs.retain(|_, r| r.printing || r.updated.map_or(true, |t| t.elapsed() < REMEMBER_PRINTED));
        let rec = jobs.entry(job_id.to_string()).or_default();
        if rec.printing {
            Claim::Busy
        } else if rec.printed {
            Claim::AlreadyPrinted
        } else {
            rec.printing = true;
            Claim::Print
        }
    }

    /// Record the outcome; returns the number of failed attempts so far.
    fn finish(&self, job_id: &str, success: bool) -> u32 {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let rec = jobs.entry(job_id.to_string()).or_default();
        rec.printing = false;
        rec.updated = Some(Instant::now());
        if success {
            rec.printed = true;
        } else {
            rec.failures += 1;
        }
        rec.failures
    }
}

/// Handles a print job from either the WebSocket push path or the polling path.
/// Returns `true` if the job is printed (now or earlier), `false` otherwise.
pub async fn handle_print_job(
    job: PrintJob,
    target: &PrintTarget,
    state: Arc<AppState>,
    agent: &crate::config::AgentConfig,
) -> bool {
    let agent_id = agent.agent_id.as_str();
    let company_code = agent.company_code.as_str();

    match state.job_ledger.claim(&job.job_id) {
        Claim::Busy => {
            log::info!("Job {} is already printing, skipping duplicate delivery", job.job_id);
            return false;
        }
        Claim::AlreadyPrinted => {
            log::info!("Job {} was already printed; re-sending its ack instead of reprinting", job.job_id);
            update_job_status_on_api(&job, "printed", agent_id, company_code, &state.app_handle).await;
            return true;
        }
        Claim::Print => {}
    }

    // Render in the printer's language (ESC/POS receipt or ZPL/TSPL label).
    let (lang, pin) = (agent.language(), agent.drawer_pin());
    let print_target = target.clone();
    let print_job = job.clone();
    let result = tokio::task::spawn_blocking(move || {
        printer::print_job_as(&print_job, &print_target, lang, pin)
    })
    .await
    .unwrap_or_else(|e| Err(format!("Print task failed: {}", e)));

    let success = result.is_ok();
    let failures = state.job_ledger.finish(&job.job_id, success);

    let status = match result {
        Ok(_) => {
            log::info!(
                "Job {} printed to '{}' successfully.",
                job.job_id,
                target.display()
            );
            "printed"
        }
        Err(e) if failures < MAX_PRINT_ATTEMPTS => {
            // Leave the job pending on the API so the next poll retries it.
            log::warn!(
                "Failed to print job {} to '{}' (attempt {}/{}), will retry: {}",
                job.job_id,
                target.display(),
                failures,
                MAX_PRINT_ATTEMPTS,
                e
            );
            return false;
        }
        Err(e) => {
            log::error!(
                "Giving up on job {} after {} attempts to '{}': {}",
                job.job_id,
                failures,
                target.display(),
                e
            );
            "failed"
        }
    };

    // Update job status on the API (both success and failure)
    update_job_status_on_api(&job, status, agent_id, company_code, &state.app_handle).await;

    // Send WebSocket ack if we have an active connection
    // (no-op on the polling path since there is no WS sender)
    let ack = serde_json::json!({
        "type":    "print_ack",
        "jobId":   job.job_id,
        "status":  status,
        "agentId": agent_id,
    })
    .to_string();

    let connections = state.agent_connections.lock().await;
    if let Some(conn) = connections.get(agent_id) {
        if let Some(tx) = conn.sender.as_ref() {
            let _ = tx.send(ack);
        }
    }

    success
}

/// Report the job's status, retrying transient failures. If every attempt
/// fails the job stays pending on the API; the ledger stops it reprinting
/// and the next poll re-sends the ack.
async fn update_job_status_on_api(
    job: &PrintJob,
    status: &str,
    agent_id: &str,
    company_code: &str,
    app_handle: &tauri::AppHandle,
) {
    let api_base = crate::commands::api_base(app_handle);

    let client = reqwest::Client::new();
    let url = format!("{}/api/agents/print-jobs/ack", api_base);

    let payload = serde_json::json!({
        "job_id":   job.job_id,
        "status":   status,
        "agent_id": agent_id,
    });

    log::info!(
        "Updating job status on API: job_id={}, status={}, agent_id={}",
        job.job_id, status, agent_id
    );

    for attempt in 1..=ACK_ATTEMPTS {
        let result = client
            .post(&url)
            .header("companycode", company_code)
            .json(&payload)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        match result {
            Ok(resp) if resp.status().is_success() => {
                let http_status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                log::info!("Job ack response ({}) — {}", http_status, body);
                return;
            }
            // A 4xx won't succeed on retry.
            Ok(resp) if resp.status().is_client_error() => {
                let http_status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                log::error!("Job ack for {} rejected ({}): {}", job.job_id, http_status, body);
                return;
            }
            Ok(resp) => log::warn!(
                "Job ack for {} failed ({}), attempt {}/{}",
                job.job_id,
                resp.status(),
                attempt,
                ACK_ATTEMPTS
            ),
            Err(e) => log::warn!(
                "Failed to send job ack for {} (attempt {}/{}): {}",
                job.job_id,
                attempt,
                ACK_ATTEMPTS,
                e
            ),
        }
        if attempt < ACK_ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(2 * attempt as u64)).await;
        }
    }
    log::error!("Could not ack job {} as '{}'; will retry on the next poll", job.job_id, status);
}

pub fn update_last_job(state: &Arc<AppState>, job_id: &str) {
    if let Ok(mut last) = state.last_job_id.lock() {
        *last = Some(job_id.to_string());
    }
    if let Ok(mut last_time) = state.last_job_time.lock() {
        *last_time = Some(chrono::Utc::now().to_rfc3339());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_prevents_duplicate_prints() {
        let ledger = JobLedger::default();
        assert_eq!(ledger.claim("j1"), Claim::Print);
        // Second delivery while the first is still printing.
        assert_eq!(ledger.claim("j1"), Claim::Busy);
        ledger.finish("j1", true);
        // Delivered again after printing (e.g. the ack was lost).
        assert_eq!(ledger.claim("j1"), Claim::AlreadyPrinted);
    }

    #[test]
    fn ledger_allows_retry_after_failure() {
        let ledger = JobLedger::default();
        assert_eq!(ledger.claim("j2"), Claim::Print);
        assert_eq!(ledger.finish("j2", false), 1);
        assert_eq!(ledger.claim("j2"), Claim::Print);
        assert_eq!(ledger.finish("j2", false), 2);
    }
}
