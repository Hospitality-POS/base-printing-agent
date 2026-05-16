use crate::config::{self, Config};
use crate::job_queue;
use crate::printer;
use crate::{start_ws_loop, AppState};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub agent_id: String,
    pub ws_connected: bool,
    pub zone: String,
    pub branch_id: String,
    pub device_name: String,
    pub queue_count: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairResponse {
    agent_id: String,
    device_token: String,
    ws_url: String,
    company_id: String,
}

#[tauri::command]
pub fn get_available_ports() -> Vec<String> {
    printer::get_available_ports()
}

#[tauri::command]
pub async fn discover_network_printers() -> Vec<String> {
    printer::discover_network_printers().await
}

#[tauri::command]
pub fn get_config(app_handle: tauri::AppHandle) -> Config {
    config::load_config(&app_handle).unwrap_or_default()
}

#[tauri::command]
pub async fn get_agent_status(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AgentStatus>, String> {
    let cfg = config::load_config(&app_handle).unwrap_or_default();
    let connections = state.agent_connections.lock().await;
    let pool_lock = state.db_pool.lock().await;
    let pool = pool_lock.as_ref().ok_or("DB pool not initialized")?;

    let mut statuses = Vec::new();
    for agent in &cfg.agents {
        let ws_connected = connections
            .get(&agent.agent_id)
            .map(|c| c.connected)
            .unwrap_or(false);

        let queue_count = job_queue::get_pending_count_for_agent(pool, &agent.agent_id).await.unwrap_or(0);

        statuses.push(AgentStatus {
            agent_id: agent.agent_id.clone(),
            ws_connected,
            zone: agent.zone.clone(),
            branch_id: agent.branch_id.clone(),
            device_name: agent.device_name.clone(),
            queue_count,
        });
    }

    Ok(statuses)
}

#[tauri::command]
pub async fn pair_device(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    company_code: String,
    branch_id: String,
    zone: String,
    device_name: String,
    printer_port: String,
    printer_type: String,
    printer_host: Option<String>,
    printer_network_port: Option<u16>,
) -> Result<(), String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());

    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "companyCode": company_code,
        "branchId": branch_id,
        "zone": zone,
        "deviceName": device_name,
    });

    let resp = client
        .post(format!("{}/api/agents/pair", api_base))
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Pairing request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Pairing failed ({}): {}", status, text));
    }

    let pair_resp: PairResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse pairing response: {}", e))?;

    let new_agent = config::AgentConfig {
        agent_id: pair_resp.agent_id,
        company_id: pair_resp.company_id,
        company_code: company_code.clone(),
        branch_id,
        zone,
        device_name,
        device_token: pair_resp.device_token,
        printer_port,
        paper_width: 80,
        ws_url: pair_resp.ws_url,
        printer_type,
        printer_host,
        printer_network_port,
    };

    config::add_agent(&app_handle, new_agent)?;

    log::info!("Agent added, starting WebSocket loop for all agents...");
    let cfg = config::load_config(&app_handle).unwrap_or_default();
    start_ws_loop(state.inner().clone(), &cfg).await;

    Ok(())
}

#[tauri::command]
pub async fn test_print(
    app_handle: tauri::AppHandle,
    _state: State<'_, Arc<AppState>>,
    agent_id: Option<String>,
    printer_type: Option<String>,
    printer_port: Option<String>,
    printer_host: Option<String>,
    printer_network_port: Option<u16>,
) -> Result<(), String> {
    let cfg = config::load_config(&app_handle).unwrap_or_default();

    let target = if let Some(aid) = agent_id {
        let agent = cfg.agents.iter().find(|a| a.agent_id == aid)
            .ok_or_else(|| format!("Agent {} not found", aid))?;
        printer::PrintTarget::from_agent_config(agent)
    } else if !cfg.agents.is_empty() {
        printer::PrintTarget::from_agent_config(&cfg.agents[0])
    } else {
        let printer_type = printer_type.unwrap_or_else(|| "serial".to_string());
        let printer_port = printer_port.unwrap_or_default();
        let printer_host = printer_host.unwrap_or_default();
        let printer_network_port = printer_network_port.unwrap_or(9100);
        
        if printer_type == "network" {
            printer::PrintTarget::Network {
                host: printer_host,
                port: printer_network_port,
            }
        } else {
            printer::PrintTarget::Serial(printer_port)
        }
    };
    
    printer::test_print(&target)
}

#[tauri::command]
pub async fn unpair_device(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<(), String> {
    config::remove_agent(&app_handle, &agent_id)?;

    let mut connections = state.agent_connections.lock().await;
    connections.remove(&agent_id);

    let cfg = config::load_config(&app_handle).unwrap_or_default();
    if cfg.agents.is_empty() {
        config::delete_config(&app_handle)?;
    }

    Ok(())
}
