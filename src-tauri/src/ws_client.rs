use crate::commands::PrintJob;
use crate::commands::PendingJob;
use crate::AppState;
use crate::printer::PrintTarget;
use crate::job_handler;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn run_ws_loop(
    state: Arc<AppState>,
    agent: crate::config::AgentConfig,
    printer_target: PrintTarget,
) {
    let ws_url = agent.ws_url.clone();
    let device_token = agent.device_token.clone();
    let agent_id = agent.agent_id.clone();
    let branch_id = agent.branch_id.clone();
    let company_code = agent.company_code.clone();
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
                    "zone": agent.zone,
                    "zones": agent.all_zones(),
                    "branchId": branch_id,
                    "companyCode": company_code,
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

                let auth_reply = timeout(Duration::from_secs(5), read.next()).await;
                match auth_reply {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        let val: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                        match val.get("type").and_then(|t| t.as_str()) {
                            Some("auth_ok") => {
                                log::info!("WebSocket authenticated successfully.");
                            }
                            Some("auth_error") => {
                                let msg = val
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("unknown");
                                log::error!(
                                    "Server rejected auth: {}. Re-pairing required.",
                                    msg
                                );
                                set_disconnected(&state, &agent_id).await;
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
                    &printer_target,
                    &agent,
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
    printer_target: &PrintTarget,
    agent: &crate::config::AgentConfig,
    state: Arc<AppState>,
) -> Result<(), String>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg_result) = {
        match timeout(Duration::from_secs(90), read.next()).await {
            Ok(item) => item,
            Err(_) => return Err("90 s idle timeout — no message received".to_string()),
        }
    } {
        match msg_result {
            Ok(Message::Text(text)) => {
                handle_message(
                    &text,
                    printer_target,
                    agent,
                    Arc::clone(&state),
                )
                .await;
            }
            Ok(Message::Ping(_)) => {
                send_via_state(&state, &agent.agent_id, r#"{"type":"pong"}"#.to_string()).await;
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

/// Decide whether a job should be printed by this agent, based on its
/// routing mode: "location" matches on the job's location/section, anything
/// else (default "category") matches on zone + optional main_category_id.
pub fn job_matches_agent(
    routing_mode: Option<&str>,
    agent_zones: &[String],
    agent_main_category_ids: &[String],
    agent_locations: &[String],
    job_zone: &str,
    job_main_category_id: Option<&str>,
    job_location: Option<&str>,
    job_location_name: Option<&str>,
) -> bool {
    match routing_mode {
        Some("location") => {
            // The agent may hold location ids in `locations` and names in
            // `zones`; jobs may carry either the id or the name.
            let agent_targets: Vec<&str> = agent_locations
                .iter()
                .chain(agent_zones.iter())
                .map(|s| s.as_str())
                .filter(|s| !s.is_empty())
                .collect();
            let candidates: Vec<&str> = [job_location, job_location_name]
                .into_iter()
                .flatten()
                .filter(|c| !c.is_empty())
                .collect();
            if !agent_targets.is_empty() && !candidates.is_empty() {
                // Anything ordered from one of this agent's locations prints
                // here, regardless of the item's main category.
                candidates.iter().any(|c| agent_targets.contains(c))
            } else {
                // Job carries no location — fall back to zone matching.
                agent_zones.iter().any(|z| z == job_zone)
            }
        }
        _ => {
            if !agent_zones.iter().any(|z| z == job_zone) {
                return false;
            }
            // When both sides know main category ids, require membership.
            match job_main_category_id {
                Some(j) if !j.is_empty() && !agent_main_category_ids.is_empty() => {
                    agent_main_category_ids.iter().any(|a| a == j)
                }
                _ => true,
            }
        }
    }
}

async fn handle_message(
    text: &str,
    printer_target: &PrintTarget,
    agent: &crate::config::AgentConfig,
    state: Arc<AppState>,
) {
    let agent_id = &agent.agent_id;
    let company_code = &agent.company_code;
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
                        if !job_matches_agent(
                            agent.routing_mode.as_deref(),
                            &agent.all_zones(),
                            &agent.all_main_category_ids(),
                            &agent.all_locations(),
                            &job.destination_zone,
                            job.main_category_id.as_deref(),
                            job.location_id.as_deref().or(job.location.as_deref()),
                            job.location_name.as_deref(),
                        ) {
                            log::info!(
                                "Job {} (zone '{}', location {:?}) does not match this agent (zone '{}', location {:?}), skipping.",
                                job.job_id,
                                job.destination_zone,
                                job.location,
                                agent.zone,
                                agent.location,
                            );
                            return;
                        }

                        job_handler::update_last_job(&state, &job.job_id);

                        // WS path: handle_print_job internally calls update_job_status_on_api
                        // and sends WS ack — no additional work needed here
                        job_handler::handle_print_job(
                            job,
                            printer_target,
                            Arc::clone(&state),
                            agent_id,
                            company_code,
                        )
                        .await;
                    }
                    Err(e) => log::error!("Failed to parse PrintJob: {}", e),
                }
            }
        }
        Some("ping") => {
            send_via_state(&state, agent_id, r#"{"type":"pong"}"#.to_string()).await;
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

pub async fn start_polling_loop(
    state: Arc<AppState>,
    shop_id: String,
    zone: String,
    company_code: String,
    agent_id: String,
) {
    log::info!(
        "Polling loop started for shop_id: {}, zone: {}, company_code: {}, agent_id: {}",
        shop_id,
        zone,
        company_code,
        agent_id
    );

    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        match fetch_and_process_jobs(&state, &shop_id, &zone, &company_code, &agent_id).await {
            Ok(0) => {} // silence empty polls
            Ok(count) => log::info!("Polling completed: {} job(s) processed", count),
            Err(e) => log::warn!("Polling failed: {}", e),
        }
    }
}

async fn fetch_and_process_jobs(
    state: &Arc<AppState>,
    shop_id: &str,
    zone: &str,
    company_code: &str,
    agent_id: &str,
) -> Result<i32, String> {
    // Always reload config so printer changes take effect without restart
    let cfg = crate::config::load_config(&state.app_handle).unwrap_or_default();
    let agent = cfg
        .agents
        .iter()
        .find(|a| a.agent_id == agent_id)
        .ok_or_else(|| format!("Agent {} not found in config", agent_id))?;

    let printer_target = build_printer_target(agent)?;

    let api_base = crate::commands::api_base(&state.app_handle);

    let client = reqwest::Client::new();

    let mut url = format!("{}/api/agents/pending-jobs?shop_id={}", api_base, shop_id);
    // In location mode — or when the agent serves multiple zones — we poll
    // the whole shop and filter client-side, since a single zone filter
    // could exclude jobs this agent should print.
    if agent.routing_mode.as_deref() != Some("location")
        && agent.all_zones().len() <= 1
        && !zone.is_empty()
    {
        url.push_str(&format!("&zone={}", urlencoding::encode(zone)));
    }

    let resp = client
        .get(&url)
        .header("companycode", company_code)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Fetch pending jobs request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Fetch pending jobs failed ({}): {}", status, text));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let response: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse response: {}", e))?;

    let jobs_array = response["jobs"]
        .as_array()
        .ok_or("Missing jobs array in response")?;

    if jobs_array.is_empty() {
        return Ok(0);
    }

    log::info!("Found {} pending job(s) for zone '{}'", jobs_array.len(), zone);

    let mut processed_count = 0;

    for job_value in jobs_array {
        match serde_json::from_value::<PendingJob>(job_value.clone()) {
            Ok(pending_job) => {
                let job_id = pending_job.job_id.clone();

                if !job_matches_agent(
                    agent.routing_mode.as_deref(),
                    &agent.all_zones(),
                    &agent.all_main_category_ids(),
                    &agent.all_locations(),
                    &pending_job.zone,
                    Some(&pending_job.main_category_id),
                    pending_job.location_id.as_deref().or(pending_job.location.as_deref()),
                    pending_job.location_name.as_deref(),
                ) {
                    log::debug!("Job {} does not match this agent's routing, skipping", job_id);
                    continue;
                }

                log::info!("Processing job: {}", job_id);

                let print_job = PrintJob {
                    id:               Some(pending_job._id.clone()),
                    job_id:           pending_job.job_id.clone(),
                    branch_id:        pending_job.shop_id.clone(),
                    destination_zone: pending_job.zone.clone(),
                    content_type:     pending_job.content_type.clone(),
                    lines:            pending_job.lines.clone(),
                    cut_paper:        pending_job.cut_paper,
                    priority:         pending_job.priority.clone(),
                    created_at:       pending_job.created_at.clone(),
                    main_category_id: Some(pending_job.main_category_id.clone()),
                    location:         pending_job.location.clone(),
                    location_id:      pending_job.location_id.clone(),
                    location_name:    pending_job.location_name.clone(),
                    table_name:       pending_job.table_name.clone(),
                };

                job_handler::update_last_job(state, &print_job.job_id);

                // handle_print_job returns true on success, false on failure.
                // It internally calls update_job_status_on_api (the /ack endpoint)
                // with "printed" or "failed" — so we do NOT call acknowledge_job
                // separately to avoid double-acking.
                let success = job_handler::handle_print_job(
                    print_job,
                    &printer_target,
                    Arc::clone(state),
                    agent_id,
                    company_code,
                )
                .await;

                if success {
                    log::info!("Job {} printed and acknowledged successfully", job_id);
                    processed_count += 1;
                } else {
                    // Job stays "failed" on the API — it will NOT be re-polled
                    // unless the server resets its status back to "pending".
                    log::warn!(
                        "Job {} failed to print — marked as failed on API",
                        job_id
                    );
                }
            }
            Err(e) => {
                log::error!("Failed to parse PendingJob: {} — raw: {}", e, job_value);
            }
        }
    }

    Ok(processed_count)
}

/// Build a PrintTarget from an AgentConfig, covering all three printer types.
fn build_printer_target(agent: &crate::config::AgentConfig) -> Result<PrintTarget, String> {
    match agent.printer_type.as_str() {
        "network" => {
            let host = agent
                .printer_host
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string();
            let port = agent.printer_network_port.unwrap_or(0);
            if host.is_empty() {
                return Err("Network printer requires a valid host address".to_string());
            }
            if port == 0 {
                return Err("Network printer requires a valid port number".to_string());
            }
            Ok(PrintTarget::Network {
                host,
                port: port as u16,
            })
        }
        "windows" => {
            let name = agent.printer_port.trim().to_string();
            if name.is_empty() {
                return Err(
                    "Windows printer requires a printer name (stored in printer_port field)"
                        .to_string(),
                );
            }
            log::info!("Using Windows printer: '{}'", name);
            Ok(PrintTarget::WindowsPrinter(name))
        }
        _ => {
            // "serial" or any unrecognised value — treat as serial
            let port = agent.printer_port.trim().to_string();
            if port.is_empty() {
                return Err(
                    "Serial printer requires a port (e.g. COM3 or /dev/ttyUSB0)".to_string(),
                );
            }
            Ok(PrintTarget::Serial(port))
        }
    }
}