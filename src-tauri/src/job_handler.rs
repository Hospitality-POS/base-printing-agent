use crate::commands::PrintJob;
use crate::printer::{self, PrintTarget};
use crate::AppState;
use std::sync::Arc;

/// Handles a print job from either the WebSocket push path or the polling path.
/// Returns `true` if the job printed successfully, `false` if it failed.
pub async fn handle_print_job(
    job: PrintJob,
    target: &PrintTarget,
    state: Arc<AppState>,
    agent_id: &str,
    company_code: &str,
) -> bool {
    let (status, success) = match printer::print_job(&job, target) {
        Ok(_) => {
            log::info!(
                "Job {} printed to '{}' successfully.",
                job.job_id,
                target.display()
            );
            ("printed", true)
        }
        Err(e) => {
            log::error!(
                "Failed to print job {} to '{}': {}",
                job.job_id,
                target.display(),
                e
            );
            ("failed", false)
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

    match client
        .post(&url)
        .header("companycode", company_code)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    {
        Ok(resp) => {
            let http_status = resp.status();
            match resp.text().await {
                Ok(body) => log::info!("Job ack response ({}) — {}", http_status, body),
                Err(e)   => log::error!("Failed to read ack response body: {}", e),
            }
        }
        Err(e) => {
            log::error!("Failed to send job ack to API: {}", e);
        }
    }
}

pub fn update_last_job(state: &Arc<AppState>, job_id: &str) {
    if let Ok(mut last) = state.last_job_id.lock() {
        *last = Some(job_id.to_string());
    }
    if let Ok(mut last_time) = state.last_job_time.lock() {
        *last_time = Some(chrono::Utc::now().to_rfc3339());
    }
}