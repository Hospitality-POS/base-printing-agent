use crate::commands::PrintJob;
use crate::printer::{self, PrintTarget};
use crate::AppState;
use std::sync::Arc;

pub async fn handle_print_job(
    job: PrintJob,
    target: &PrintTarget,
    state: Arc<AppState>,
    agent_id: &str,
    company_code: &str,
) {
    let status = match printer::print_job(&job, target) {
        Ok(_) => {
            log::info!("Job {} printed to {} successfully.", job.job_id, target.display());
            update_job_status_on_api(&job, "printed", agent_id, company_code).await;
            "printed"
        }
        Err(e) => {
            log::error!("Failed to print job {}: {}", job.job_id, e);
            update_job_status_on_api(&job, "failed", agent_id, company_code).await;
            "failed"
        }
    };

    let ack = serde_json::json!({
        "type": "print_ack",
        "jobId": job.job_id,
        "status": status,
        "agentId": agent_id,
    })
    .to_string();

    let connections = state.agent_connections.lock().await;
    if let Some(conn) = connections.get(agent_id) {
        if let Some(tx) = conn.sender.as_ref() {
            let _ = tx.send(ack);
        }
    }
}

async fn update_job_status_on_api(job: &crate::commands::PrintJob, status: &str, agent_id: &str, company_code: &str) {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());
    let client = reqwest::Client::new();

    let url = format!("{}/api/agents/print-jobs/ack", api_base);

    let payload = serde_json::json!({
        "job_id": job.job_id,
        "status": status,
        "agent_id": agent_id,
    });

    log::info!("Updating job status on API: {} -> {} (job_id: {})",
        job.job_id, status, job.job_id);
    log::info!("API URL: {}", url);
    log::info!("Payload: {}", payload);

    match client
        .post(&url)
        .header("companycode", company_code)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    {
        Ok(response) => {
            log::info!("API response status: {}", response.status());
            match response.text().await {
                Ok(body) => log::info!("API response body: {}", body),
                Err(e) => log::error!("Failed to read API response: {}", e),
            }
        }
        Err(e) => {
            log::error!("Failed to update job status on API: {}", e);
        }
    }
}

pub fn update_last_job(state: &Arc<AppState>, job_id: &str) {
    let mut last = state.last_job_id.lock().unwrap();
    *last = Some(job_id.to_string());

    let mut last_time = state.last_job_time.lock().unwrap();
    *last_time = Some(chrono::Utc::now().to_rfc3339());
}
