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
    pub ws_connected: bool,
    pub zone: String,
    pub branch_id: String,
    pub last_job_id: Option<String>,
    pub last_job_time: Option<String>,
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
pub fn get_config(app_handle: tauri::AppHandle) -> Option<Config> {
    config::load_config(&app_handle)
}

#[tauri::command]
pub async fn get_agent_status(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AgentStatus, String> {
    let cfg = config::load_config(&app_handle);

    let ws_connected = {
        let lock = state.ws_connected.lock().map_err(|e| e.to_string())?;
        *lock
    };

    let last_job_id = {
        let lock = state.last_job_id.lock().map_err(|e| e.to_string())?;
        lock.clone()
    };

    let last_job_time = {
        let lock = state.last_job_time.lock().map_err(|e| e.to_string())?;
        lock.clone()
    };

    let pool_opt = {
        let pool_lock = state.db_pool.lock().await;
        pool_lock.as_ref().map(|p| p.clone())
    };
    let queue_count = match pool_opt {
        Some(pool) => job_queue::get_pending_count(&pool).await,
        None => 0,
    };

    Ok(AgentStatus {
        ws_connected,
        zone: cfg.as_ref().map(|c| c.zone.clone()).unwrap_or_default(),
        branch_id: cfg.as_ref().map(|c| c.branch_id.clone()).unwrap_or_default(),
        last_job_id,
        last_job_time,
        queue_count,
    })
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
        .unwrap_or_else(|_| "https://your-saas.com".to_string());

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

    let new_config = Config {
        agent_id: pair_resp.agent_id,
        company_id: pair_resp.company_id,
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

    config::save_config(&app_handle, &new_config)?;

    {
        let mut cfg_lock = state.current_config.lock().await;
        *cfg_lock = Some(new_config.clone());
    }

    start_ws_loop(state.inner().clone(), &new_config).await;

    Ok(())
}

#[tauri::command]
pub async fn test_print(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    printer_type: Option<String>,
    printer_port: Option<String>,
    printer_host: Option<String>,
    printer_network_port: Option<u16>,
) -> Result<(), String> {
    let target = if let Some(pt) = printer_type {
        if pt == "network" {
            let host = printer_host.unwrap_or_default();
            if host.is_empty() {
                return Err("Printer IP address is required".to_string());
            }
            printer::PrintTarget::Network {
                host,
                port: printer_network_port.unwrap_or(9100),
            }
        } else {
            let port = printer_port.unwrap_or_default();
            if port.is_empty() {
                return Err("Printer port is required".to_string());
            }
            printer::PrintTarget::Serial(port)
        }
    } else {
        let cfg = {
            let cfg_lock = state.current_config.lock().await;
            cfg_lock.clone()
        };
        let cfg = match cfg {
            Some(c) => c,
            None => config::load_config(&app_handle)
                .ok_or_else(|| "No printer configured".to_string())?,
        };
        printer::PrintTarget::from_config(&cfg)
    };
    printer::test_print(&target)
}

#[tauri::command]
pub async fn unpair_device(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    config::delete_config(&app_handle)?;

    let mut cfg_lock = state.current_config.lock().await;
    *cfg_lock = None;

    let mut connected = state.ws_connected.lock().map_err(|e| e.to_string())?;
    *connected = false;

    Ok(())
}
