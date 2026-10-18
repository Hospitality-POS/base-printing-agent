//! Non-print device features layered on top of the printer agents: cash
//! drawer, printer status monitoring, weighing scales and label printing.
//! Used by the Tauri commands, the cloud WebSocket and the local HTTP API.

use crate::config::{self, AgentConfig, Config};
use crate::printer::{self, PrintTarget, PrinterStatus, ScaleConfig, WeightReading};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the background monitor checks each printer.
const STATUS_INTERVAL: Duration = Duration::from_secs(60);
/// Re-send an unchanged status to the server at least this often.
const STATUS_REPORT_EVERY: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    pub agent_id: String,
    pub status: PrinterStatus,
    pub checked_at: String,
}

pub fn find_agent<'a>(cfg: &'a Config, agent_id: &str) -> Result<&'a AgentConfig, String> {
    cfg.agents
        .iter()
        .find(|a| a.agent_id == agent_id)
        .ok_or_else(|| format!("Printer {} is not set up on this computer", agent_id))
}

/// The printer whose drawer to open: the one asked for, else the first one
/// marked as having a cash drawer, else the only receipt printer.
pub fn drawer_agent<'a>(cfg: &'a Config, agent_id: Option<&str>) -> Result<&'a AgentConfig, String> {
    if let Some(id) = agent_id.filter(|id| !id.is_empty()) {
        return find_agent(cfg, id);
    }
    if let Some(a) = cfg.agents.iter().find(|a| a.cash_drawer) {
        return Ok(a);
    }
    let receipt: Vec<&AgentConfig> = cfg.agents.iter().filter(|a| !a.language().is_label()).collect();
    match receipt.as_slice() {
        [only] => Ok(*only),
        [] => Err("No receipt printer is set up on this computer".into()),
        _ => Err("Several printers are set up — mark the one with the cash drawer, or pass agentId".into()),
    }
}

pub fn target_for(agent: &AgentConfig) -> Result<PrintTarget, String> {
    crate::ws_client::build_printer_target(agent)
}

pub fn find_scale<'a>(cfg: &'a Config, scale_id: Option<&str>) -> Result<&'a ScaleConfig, String> {
    match scale_id.filter(|id| !id.is_empty() && *id != "default") {
        Some(id) => cfg
            .scales
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("Scale {} is not set up on this computer", id)),
        None => cfg.scales.first().ok_or_else(|| "No scale is set up on this computer".to_string()),
    }
}

fn load(state: &AppState) -> Config {
    config::load_config(&state.app_handle).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Blocking operations (call from spawn_blocking or a std thread)
// ---------------------------------------------------------------------------

pub fn open_drawer_blocking(state: &AppState, agent_id: Option<&str>) -> Result<String, String> {
    let cfg = load(state);
    let agent = drawer_agent(&cfg, agent_id)?;
    if agent.language().is_label() {
        return Err(format!("{} is a label printer and has no cash drawer port", agent.device_name));
    }
    printer::open_cash_drawer(&target_for(agent)?, agent.drawer_pin())?;
    log::info!("Cash drawer opened via {}", agent.device_name);
    Ok(agent.agent_id.clone())
}

pub fn status_blocking(state: &AppState, agent_id: &str) -> Result<StatusEntry, String> {
    let cfg = load(state);
    let agent = find_agent(&cfg, agent_id)?;
    let status = match target_for(agent) {
        Ok(t) => printer::status::query_status_for(&t, agent.language()),
        Err(e) => PrinterStatus {
            state: "unknown".into(),
            source: "none".into(),
            messages: vec![e],
            ..Default::default()
        },
    };
    let entry = StatusEntry {
        agent_id: agent_id.to_string(),
        status,
        checked_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Ok(mut map) = state.printer_status.lock() {
        map.insert(agent_id.to_string(), entry.clone());
    }
    Ok(entry)
}

pub fn cached_statuses(state: &AppState) -> std::collections::HashMap<String, StatusEntry> {
    state.printer_status.lock().map(|m| m.clone()).unwrap_or_default()
}

pub fn read_scale_blocking(state: &AppState, scale_id: Option<&str>) -> Result<(String, WeightReading), String> {
    let cfg = load(state);
    let scale = find_scale(&cfg, scale_id)?;
    printer::scale::read_weight(scale).map(|r| (scale.id.clone(), r))
}

pub fn scale_command_blocking(state: &AppState, scale_id: Option<&str>, command: &str) -> Result<(), String> {
    let cfg = load(state);
    printer::scale::send_command(find_scale(&cfg, scale_id)?, command)
}

pub fn print_label_blocking(state: &AppState, agent_id: Option<&str>, label: &printer::LabelJob) -> Result<String, String> {
    let cfg = load(state);
    let agent = match agent_id.filter(|id| !id.is_empty()) {
        Some(id) => find_agent(&cfg, id)?,
        None => cfg
            .agents
            .iter()
            .find(|a| a.language().is_label())
            .ok_or("No label printer (ZPL/TSPL) is set up on this computer")?,
    };
    printer::print_label(label, &target_for(agent)?, agent.language())?;
    Ok(agent.agent_id.clone())
}

pub fn print_job_blocking(state: &AppState, agent_id: &str, job: &printer::PrintJob) -> Result<(), String> {
    let cfg = load(state);
    let agent = find_agent(&cfg, agent_id)?;
    printer::print_job_as(job, &target_for(agent)?, agent.language(), agent.drawer_pin())
}

// ---------------------------------------------------------------------------
// Async wrappers
// ---------------------------------------------------------------------------

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| format!("Device task failed: {}", e))
}

pub async fn refresh_status(state: Arc<AppState>, agent_id: String) -> Result<StatusEntry, String> {
    blocking(move || status_blocking(&state, &agent_id)).await?
}

/// What this agent can do, sent with the WebSocket auth message so the
/// server knows which device commands it may send.
pub fn capabilities(agent: &AgentConfig, cfg: &Config) -> serde_json::Value {
    let lang = agent.language();
    let mut caps = vec!["print", "status"];
    if lang.is_label() {
        caps.push("label");
    } else if agent.cash_drawer {
        caps.push("drawer");
    }
    if !cfg.scales.is_empty() {
        caps.push("scale");
    }
    serde_json::json!({
        "capabilities": caps,
        "printerLanguage": lang.as_str(),
        "cashDrawer": agent.cash_drawer,
        "scales": cfg.scales.iter().map(|s| serde_json::json!({
            "id": s.id, "name": s.name, "protocol": s.protocol,
        })).collect::<Vec<_>>(),
        "localApiPort": cfg.local_api.enabled.then_some(cfg.local_api.port),
    })
}

// ---------------------------------------------------------------------------
// Background status monitor
// ---------------------------------------------------------------------------

pub async fn run_status_monitor(state: Arc<AppState>, agent_id: String) {
    let mut last_seen: Option<String> = None;
    // Last state the server acknowledged receiving (None = never sent).
    let mut last_sent: Option<(String, Instant)> = None;
    // Let startup (WS connect, first poll) settle before touching the port.
    tokio::time::sleep(Duration::from_secs(5)).await;
    loop {
        match refresh_status(Arc::clone(&state), agent_id.clone()).await {
            Ok(entry) => {
                let current = entry.status.state.clone();
                if last_seen.as_deref() != Some(current.as_str()) {
                    log::info!("Printer {} status: {} {:?}", agent_id, current, entry.status.messages);
                    last_seen = Some(current.clone());
                }
                let due = match &last_sent {
                    Some((s, at)) => *s != current || at.elapsed() >= STATUS_REPORT_EVERY,
                    None => true,
                };
                if due {
                    let msg = status_message(&agent_id, None, &entry);
                    // When offline from the server, retry on the next tick.
                    if crate::ws_client::send_via_state(&state, &agent_id, msg).await {
                        last_sent = Some((current, Instant::now()));
                    }
                }
            }
            Err(e) => log::debug!("Status check for {} skipped: {}", agent_id, e),
        }
        tokio::time::sleep(STATUS_INTERVAL).await;
    }
}

fn status_message(agent_id: &str, request_id: Option<&str>, entry: &StatusEntry) -> String {
    serde_json::json!({
        "type": "printer_status",
        "agentId": agent_id,
        "requestId": request_id,
        "status": entry.status,
        "checkedAt": entry.checked_at,
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Cloud WebSocket device commands
// ---------------------------------------------------------------------------

/// Handle a device message from the server. Returns false for unknown types.
pub async fn handle_ws_message(msg_type: &str, value: &serde_json::Value, agent: &AgentConfig, state: Arc<AppState>) -> bool {
    let request_id = value.get("requestId").and_then(|v| v.as_str()).map(str::to_string);
    let agent_id = agent.agent_id.clone();

    let reply = match msg_type {
        "open_drawer" => {
            let st = Arc::clone(&state);
            let id = agent_id.clone();
            let result = blocking(move || open_drawer_blocking(&st, Some(&id))).await.and_then(|r| r);
            serde_json::json!({
                "type": "drawer_ack",
                "agentId": agent_id,
                "requestId": request_id,
                "status": if result.is_ok() { "opened" } else { "failed" },
                "error": result.err(),
            })
            .to_string()
        }
        "printer_status_request" => match refresh_status(Arc::clone(&state), agent_id.clone()).await {
            Ok(entry) => status_message(&agent_id, request_id.as_deref(), &entry),
            Err(e) => serde_json::json!({
                "type": "printer_status", "agentId": agent_id, "requestId": request_id, "error": e,
            })
            .to_string(),
        },
        "scale_read" => {
            let st = Arc::clone(&state);
            let scale_id = value.get("scaleId").and_then(|v| v.as_str()).map(str::to_string);
            let result = blocking(move || read_scale_blocking(&st, scale_id.as_deref())).await.and_then(|r| r);
            match result {
                Ok((scale_id, reading)) => serde_json::json!({
                    "type": "scale_reading", "agentId": agent_id, "requestId": request_id,
                    "scaleId": scale_id, "reading": reading,
                }),
                Err(e) => serde_json::json!({
                    "type": "scale_reading", "agentId": agent_id, "requestId": request_id, "error": e,
                }),
            }
            .to_string()
        }
        "scale_command" => {
            let st = Arc::clone(&state);
            let scale_id = value.get("scaleId").and_then(|v| v.as_str()).map(str::to_string);
            let command = value.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let cmd = command.clone();
            let result = blocking(move || scale_command_blocking(&st, scale_id.as_deref(), &cmd)).await.and_then(|r| r);
            serde_json::json!({
                "type": "scale_command_ack", "agentId": agent_id, "requestId": request_id,
                "command": command, "status": if result.is_ok() { "ok" } else { "failed" }, "error": result.err(),
            })
            .to_string()
        }
        "print_label" => {
            let label = value.get("label").cloned().map(serde_json::from_value::<printer::LabelJob>);
            let result = match label {
                Some(Ok(label)) => {
                    let st = Arc::clone(&state);
                    let id = agent_id.clone();
                    blocking(move || print_label_blocking(&st, Some(&id), &label)).await.and_then(|r| r).map(|_| ())
                }
                Some(Err(e)) => Err(format!("Invalid label: {}", e)),
                None => Err("Missing 'label'".to_string()),
            };
            serde_json::json!({
                "type": "label_ack", "agentId": agent_id, "requestId": request_id,
                "status": if result.is_ok() { "printed" } else { "failed" }, "error": result.err(),
            })
            .to_string()
        }
        _ => return false,
    };

    crate::ws_client::send_via_state(&state, &agent.agent_id, reply).await;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, lang: Option<&str>, drawer: bool) -> AgentConfig {
        serde_json::from_value(serde_json::json!({
            "agentId": id, "companyId": "", "companyCode": "", "branchId": "", "zone": "",
            "deviceName": id, "deviceToken": "", "printerPort": "COM1", "paperWidth": 80, "wsUrl": "",
            "printerLanguage": lang, "cashDrawer": drawer,
        }))
        .unwrap()
    }

    #[test]
    fn picks_drawer_printer() {
        let mut cfg = Config::default();
        cfg.agents = vec![agent("a", None, false), agent("label", Some("zpl"), false)];
        // Only one receipt printer → that one.
        assert_eq!(drawer_agent(&cfg, None).unwrap().agent_id, "a");
        cfg.agents.push(agent("b", None, true));
        assert_eq!(drawer_agent(&cfg, None).unwrap().agent_id, "b");
        assert_eq!(drawer_agent(&cfg, Some("a")).unwrap().agent_id, "a");
        cfg.agents[2].cash_drawer = false;
        assert!(drawer_agent(&cfg, None).is_err());
    }
}
