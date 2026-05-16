use crate::job_handler;
use crate::job_queue::PrintJob;
use crate::printer::PrintTarget;
use crate::AppState;
use futures_util::{SinkExt, StreamExt};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn run_ws_loop(
    state: Arc<AppState>,
    pool: SqlitePool,
    ws_url: String,
    device_token: String,
    agent_id: String,
    zone: String,
    branch_id: String,
    company_id: String,
    printer_target: PrintTarget,
) {
    loop {
        log::info!("Connecting to WebSocket at {}", ws_url);

        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                log::info!("WebSocket connected for agent {}", agent_id);
                {
                    let mut connections = state.agent_connections.lock().await;
                    connections.entry(agent_id.clone()).or_insert_with(|| crate::AgentConnection {
                        connected: true,
                        sender: None,
                    });
                }

                let (tx, rx): (UnboundedSender<String>, UnboundedReceiver<String>) =
                    mpsc::unbounded_channel();

                {
                    let mut connections = state.agent_connections.lock().await;
                    if let Some(conn) = connections.get_mut(&agent_id) {
                        conn.sender = Some(tx.clone());
                    }
                }

                let auth_msg = serde_json::json!({
                    "type": "auth",
                    "deviceToken": device_token,
                    "agentId": agent_id,
                    "zone": zone,
                    "branchId": branch_id,
                    "companyCode": company_id,
                })
                .to_string();

                let (mut write, mut read) = ws_stream.split();

                if let Err(e) = write.send(Message::Text(auth_msg)).await {
                    log::error!("Failed to send auth message: {}", e);
                    set_disconnected(&state, &agent_id).await;
                    sleep(Duration::from_secs(3)).await;
                    continue;
                }

                log::info!("Auth message sent. Waiting for auth_ok...");

                // Wait up to 5 s for auth_ok from server
                let auth_reply = timeout(Duration::from_secs(5), read.next()).await;
                match auth_reply {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        let val: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                        match val.get("type").and_then(|t| t.as_str()) {
                            Some("auth_ok") => {
                                log::info!("WebSocket authenticated successfully.");
                            }
                            Some("auth_error") => {
                                let msg = val.get("message").and_then(|m| m.as_str()).unwrap_or("unknown");
                                log::error!("Server rejected auth: {}. Re-pairing required.", msg);
                                set_disconnected(&state, &agent_id).await;
                                // Back off for 60 s — bad token won't fix itself on retry
                                sleep(Duration::from_secs(60)).await;
                                continue;
                            }
                            _ => {
                                log::warn!("Expected auth_ok, got: {}", text);
                                set_disconnected(&state, &agent_id).await;
                                sleep(Duration::from_secs(3)).await;
                                continue;
                            }
                        }
                    }
                    Ok(Some(Err(e))) => {
                        log::warn!("Connection error during auth handshake: {}", e);
                        set_disconnected(&state, &agent_id).await;
                        sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                    Ok(None) | Err(_) => {
                        log::error!("Auth timeout or no auth_ok received");
                        set_disconnected(&state, &agent_id).await;
                        sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                    _ => {
                        log::warn!("Unexpected message during auth handshake.");
                        set_disconnected(&state, &agent_id).await;
                        sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                }

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

                set_disconnected(&state, &agent_id).await;
                log::info!("WebSocket disconnected. Reconnecting in 3s...");
            }
            Err(e) => {
                log::warn!("WebSocket connection failed: {}. Retrying in 3s...", e);
                set_disconnected(&state, &agent_id).await;
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
    while let Some(msg_result) = {
        // 90 s idle timeout — reconnect if server goes silent
        match timeout(Duration::from_secs(90), read.next()).await {
            Ok(item) => item,
            Err(_) => return Err("90 s idle timeout — no message received".to_string()),
        }
    } {
        match msg_result {
            Ok(Message::Text(text)) => {
                handle_message(&text, pool, printer_target, agent_id, zone, Arc::clone(&state)).await;
            }
            Ok(Message::Ping(_)) => {
                send_via_state(&state, &agent_id, r#"{"type":"pong"}"#.to_string()).await;
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
            send_via_state(&state, &agent_id, r#"{"type":"pong"}"#.to_string()).await;
        }
        Some(t) => {
            log::debug!("Unhandled WS message type: {}", t);
        }
        None => {
            log::warn!("WS message missing 'type' field.");
        }
    }
}

async fn send_via_state(state: &Arc<AppState>, agent_id: &str, msg: String) {
    let connections = state.agent_connections.lock().await;
    if let Some(conn) = connections.get(agent_id) {
        if let Some(tx) = conn.sender.as_ref() {
            let _ = tx.send(msg);
        }
    }
}

async fn set_disconnected(state: &Arc<AppState>, agent_id: &str) {
    let mut connections = state.agent_connections.lock().await;
    if let Some(conn) = connections.get_mut(agent_id) {
        conn.connected = false;
    }
}
