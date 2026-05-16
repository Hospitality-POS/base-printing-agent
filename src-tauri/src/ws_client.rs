use crate::job_handler;
use crate::job_queue::PrintJob;
use crate::printer::PrintTarget;
use crate::AppState;
use futures_util::{SinkExt, StreamExt};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn run_ws_loop(
    state: Arc<AppState>,
    pool: SqlitePool,
    ws_url: String,
    device_token: String,
    agent_id: String,
    zone: String,
    branch_id: String,
    printer_target: PrintTarget,
) {
    loop {
        log::info!("Connecting to WebSocket at {}", ws_url);

        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                log::info!("WebSocket connected.");
                {
                    let mut connected = state.ws_connected.lock().unwrap();
                    *connected = true;
                }

                let (tx, rx): (UnboundedSender<String>, UnboundedReceiver<String>) =
                    mpsc::unbounded_channel();

                {
                    let mut sender = state.ws_sender.lock().await;
                    *sender = Some(tx);
                }

                let auth_msg = serde_json::json!({
                    "type": "auth",
                    "deviceToken": device_token,
                    "agentId": agent_id,
                    "zone": zone,
                    "branchId": branch_id,
                })
                .to_string();

                let (mut write, mut read) = ws_stream.split();

                if let Err(e) = write.send(Message::Text(auth_msg)).await {
                    log::error!("Failed to send auth message: {}", e);
                    set_disconnected(&state).await;
                    sleep(Duration::from_secs(3)).await;
                    continue;
                }

                log::info!("Auth message sent.");

                job_handler::drain_pending_queue(&pool, &printer_target, Arc::clone(&state), &agent_id).await;

                let write = Arc::new(Mutex::new(write));
                let write_clone = Arc::clone(&write);

                let outbound_task = tokio::spawn(async move {
                    let mut rx = rx;
                    while let Some(msg) = rx.recv().await {
                        let mut w = write_clone.lock().await;
                        if w.send(Message::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                });

                let inbound_result = handle_inbound(
                    &mut read,
                    &pool,
                    &printer_target,
                    &agent_id,
                    &zone,
                    Arc::clone(&state),
                )
                .await;

                outbound_task.abort();

                if let Err(e) = inbound_result {
                    log::warn!("WebSocket inbound error: {}", e);
                }

                set_disconnected(&state).await;
                log::info!("WebSocket disconnected. Reconnecting in 3s...");
            }
            Err(e) => {
                log::warn!("WebSocket connection failed: {}. Retrying in 3s...", e);
                set_disconnected(&state).await;
            }
        }

        sleep(Duration::from_secs(3)).await;
    }
}

async fn handle_inbound<S>(
    read: &mut S,
    pool: &SqlitePool,
    printer_target: &PrintTarget,
    agent_id: &str,
    zone: &str,
    state: Arc<AppState>,
) -> Result<(), String>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg_result) = read.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                handle_message(&text, pool, printer_target, agent_id, zone, Arc::clone(&state)).await;
            }
            Ok(Message::Ping(_)) => {
                send_via_state(&state, r#"{"type":"pong"}"#.to_string()).await;
            }
            Ok(Message::Close(_)) => {
                return Err("Server closed connection.".to_string());
            }
            Err(e) => {
                return Err(e.to_string());
            }
            _ => {}
        }
    }
    Ok(())
}

async fn handle_message(
    text: &str,
    pool: &SqlitePool,
    printer_target: &PrintTarget,
    agent_id: &str,
    zone: &str,
    state: Arc<AppState>,
) {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Failed to parse WS message: {}", e);
            return;
        }
    };

    match value.get("type").and_then(|t| t.as_str()) {
        Some("print_job") => {
            if let Some(job_val) = value.get("job") {
                match serde_json::from_value::<PrintJob>(job_val.clone()) {
                    Ok(job) => {
                        if job.destination_zone != zone {
                            log::info!(
                                "Job {} is for zone '{}', we are '{}', skipping.",
                                job.job_id,
                                job.destination_zone,
                                zone
                            );
                            return;
                        }

                        job_handler::update_last_job(&state, &job.job_id);

                        job_handler::handle_print_job(
                            job,
                            pool,
                            printer_target,
                            Arc::clone(&state),
                            agent_id,
                        )
                        .await;
                    }
                    Err(e) => log::error!("Failed to parse PrintJob: {}", e),
                }
            }
        }
        Some("ping") => {
            send_via_state(&state, r#"{"type":"pong"}"#.to_string()).await;
        }
        Some(t) => {
            log::debug!("Unhandled WS message type: {}", t);
        }
        None => {
            log::warn!("WS message missing 'type' field.");
        }
    }
}

async fn send_via_state(state: &Arc<AppState>, msg: String) {
    let sender = state.ws_sender.lock().await;
    if let Some(tx) = sender.as_ref() {
        let _ = tx.send(msg);
    }
}

async fn set_disconnected(state: &Arc<AppState>) {
    {
        let mut connected = state.ws_connected.lock().unwrap();
        *connected = false;
    }
    let mut sender = state.ws_sender.lock().await;
    *sender = None;
}
