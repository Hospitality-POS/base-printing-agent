use crate::job_queue::{self, PrintJob};
use crate::printer;
use crate::AppState;
use sqlx::SqlitePool;
use std::sync::Arc;

pub async fn handle_print_job(
    job: PrintJob,
    pool: &SqlitePool,
    printer_port: &str,
    state: Arc<AppState>,
    agent_id: &str,
) {
    if job_queue::is_already_printed(pool, &job.job_id).await {
        log::info!("Job {} already printed, skipping.", job.job_id);
        return;
    }

    if let Err(e) = job_queue::insert_job(pool, &job).await {
        log::error!("Failed to queue job {}: {}", job.job_id, e);
        return;
    }

    let status = match printer::print_job(&job, printer_port) {
        Ok(_) => {
            log::info!("Job {} printed successfully.", job.job_id);
            let _ = job_queue::mark_printed(pool, &job.job_id).await;
            "printed"
        }
        Err(e) => {
            log::error!("Failed to print job {}: {}", job.job_id, e);
            let _ = job_queue::mark_failed(pool, &job.job_id).await;
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

    let sender = state.ws_sender.lock().await;
    if let Some(tx) = sender.as_ref() {
        let _ = tx.send(ack);
    }
}

pub async fn drain_pending_queue(
    pool: &SqlitePool,
    printer_port: &str,
    state: Arc<AppState>,
    agent_id: &str,
) {
    let pending = job_queue::get_pending_jobs(pool).await;
    if !pending.is_empty() {
        log::info!("Draining {} pending jobs from queue...", pending.len());
    }
    for job in pending {
        handle_print_job(job, pool, printer_port, Arc::clone(&state), agent_id).await;
    }
}

pub fn update_last_job(state: &Arc<AppState>, job_id: &str) {
    let mut last = state.last_job_id.lock().unwrap();
    *last = Some(job_id.to_string());

    let mut last_time = state.last_job_time.lock().unwrap();
    *last_time = Some(chrono::Utc::now().to_rfc3339());
}
