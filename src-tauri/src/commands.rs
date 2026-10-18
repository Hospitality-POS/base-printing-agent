use crate::config::{self, Config};
use crate::devices;
use crate::printer;
use crate::AppState;
use crate::start_ws_loop;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

pub use printer_core::{PrintJob, PrintJobLine};

pub const LIVE_API_URL: &str = "https://api.hospitality.reliatech.co.ke";

/// API base URL resolution order: user-selected URL in config (Local/Live
/// toggle) -> PRINT_AGENT_API_URL env -> live default.
pub fn api_base(app_handle: &tauri::AppHandle) -> String {
    config::load_config(app_handle)
        .and_then(|c| c.api_url)
        .or_else(|| std::env::var("PRINT_AGENT_API_URL").ok())
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| LIVE_API_URL.to_string())
}

#[tauri::command]
pub fn get_api_url(app_handle: tauri::AppHandle) -> String {
    api_base(&app_handle)
}

#[tauri::command]
pub fn set_api_url(app_handle: tauri::AppHandle, url: Option<String>) -> Result<(), String> {
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();
    let cleaned = url
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty());
    cfg.api_url = cleaned;
    config::save_config(&app_handle, &cfg)
}

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

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerAgent {
    #[serde(rename = "_id", default)]
    pub _id: Option<String>,
    #[serde(rename = "agent_id")]
    pub agent_id: String,
    #[serde(rename = "company_code")]
    pub company_code: String,
    #[serde(rename = "company_id")]
    pub company_id: String,
    #[serde(rename = "shop_id")]
    pub shop_id: String,
    #[serde(rename = "zone")]
    pub zone: String,
    #[serde(rename = "device_name")]
    pub device_name: String,
    #[serde(rename = "is_active")]
    pub is_active: bool,
    #[serde(rename = "last_seen", default)]
    pub last_seen: Option<String>,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
    #[serde(rename = "printer_port", default)]
    pub printer_port: Option<String>,
    #[serde(rename = "printer_type", default)]
    pub printer_type: Option<String>,
    #[serde(rename = "printer_host", default)]
    pub printer_host: Option<String>,
    #[serde(rename = "printer_network_port", default)]
    pub printer_network_port: Option<i64>,
    #[serde(rename = "__v", default)]
    pub __v: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectedAgentsResponse {
    agents: Vec<ServerAgent>,
    count: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListAgentsResponse {
    agents: Vec<ServerAgent>,
}

#[tauri::command]
pub fn get_available_ports() -> Vec<String> {
    printer::get_available_ports()
}

#[tauri::command]
pub fn detect_thermal_printers() -> Vec<printer::PortInfo> {
    printer::detect_thermal_printers()
}

#[tauri::command]
pub fn list_windows_printers() -> Vec<String> {
    printer::list_windows_printers()
}

#[tauri::command]
pub async fn discover_network_printers() -> Vec<String> {
    tokio::task::spawn_blocking(printer::discover_network_printers)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_os_version() -> printer::OsInfo {
    printer::get_os_info()
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "PrintAgent";

#[tauri::command]
pub fn get_autostart() -> bool {
    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new("reg")
            .args(["query", RUN_KEY, "/v", RUN_VALUE])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if enabled {
            let exe = std::env::current_exe()
                .map(|p| p.display().to_string())
                .map_err(|e| e.to_string())?;
            let out = Command::new("reg")
                .args([
                    "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ",
                    "/d", &format!("\"{}\"", exe), "/f",
                ])
                .output()
                .map_err(|e| format!("Failed to run reg: {}", e))?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
        } else {
            let _ = Command::new("reg")
                .args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])
                .output();
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Autostart is only supported on Windows".to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Shop {
    pub _id: String,
    pub name: String,
    #[serde(rename = "shop_code")]
    pub shop_code: Option<String>,
}

#[tauri::command]
pub async fn fetch_shops(app_handle: tauri::AppHandle, company_code: String) -> Result<String, String> {
    let api_base = api_base(&app_handle);

    let client = reqwest::Client::new();
    let url = format!("{}/api/shops", api_base);

    let resp = client
        .get(&url)
        .header("companycode", &company_code)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Fetch shops request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Fetch shops failed ({}): {}", status, text));
    }

    let shops: Vec<Shop> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse shops response: {}", e))?;

    serde_json::to_string(&shops)
        .map_err(|e| format!("Failed to serialize shops: {}", e))
}

/// Fetch selectable options for job routing. Paths are configurable via env
/// vars because the exact backend routes may differ per deployment:
///   PRINT_AGENT_CATEGORIES_PATH (default: /api/main-categories)
///   PRINT_AGENT_LOCATIONS_PATH  (default: /api/locations)
async fn fetch_routing_options(app_handle: &tauri::AppHandle, path_env: &str, default_path: &str, shop_id: &str, company_code: &str) -> Result<String, String> {
    let api_base = api_base(app_handle);
    let path = std::env::var(path_env).unwrap_or_else(|_| default_path.to_string());
    let sep = if path.contains('?') { "&" } else { "?" };
    let url = format!("{}{}{}{}", api_base, path, sep, format!("shop_id={}", urlencoding::encode(shop_id)));

    let resp = reqwest::Client::new()
        .get(&url)
        .header("companycode", company_code)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Request to {} failed: {}", url, e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Fetch failed ({}): {}", status, text));
    }

    resp.text().await.map_err(|e| format!("Failed to read body: {}", e))
}

/// Fallback when no dedicated list endpoint exists: derive distinct option
/// values from pending print jobs for the shop.
async fn derive_options_from_jobs(app_handle: &tauri::AppHandle, shop_id: &str, company_code: &str, field: &str) -> Result<String, String> {
    let api_base = api_base(app_handle);
    let url = format!("{}/api/agents/pending-jobs?shop_id={}", api_base, urlencoding::encode(shop_id));

    let resp = reqwest::Client::new()
        .get(&url)
        .header("companycode", company_code)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Pending jobs request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Pending jobs failed ({}): {}", status, text));
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse pending jobs: {}", e))?;
    let jobs = body["jobs"].as_array().cloned().unwrap_or_default();

    let mut seen = std::collections::BTreeSet::new();
    for job in &jobs {
        if let Some(v) = job[field].as_str() {
            if !v.is_empty() {
                seen.insert(v.to_string());
            }
        }
    }

    let options: Vec<serde_json::Value> = seen
        .into_iter()
        .map(|v| serde_json::json!({ "_id": v, "name": v }))
        .collect();
    serde_json::to_string(&options).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_categories(app_handle: tauri::AppHandle, shop_id: String, company_code: String) -> Result<String, String> {
    match fetch_routing_options(
        &app_handle,
        "PRINT_AGENT_CATEGORIES_PATH",
        "/categories/main-categories",
        &shop_id,
        &company_code,
    )
    .await
    {
        Ok(s) => Ok(s),
        Err(_) => derive_options_from_jobs(&app_handle, &shop_id, &company_code, "zone").await,
    }
}

#[tauri::command]
pub async fn fetch_locations(app_handle: tauri::AppHandle, shop_id: String, company_code: String) -> Result<String, String> {
    match fetch_routing_options(
        &app_handle,
        "PRINT_AGENT_LOCATIONS_PATH",
        "/tables/location/locations",
        &shop_id,
        &company_code,
    )
    .await
    {
        Ok(s) => Ok(s),
        Err(_) => derive_options_from_jobs(&app_handle, &shop_id, &company_code, "location").await,
    }
}

#[tauri::command]
pub async fn list_agents(
    app_handle: tauri::AppHandle,
    company_code: String,
    branch_id: String,
    zone: Option<String>,
) -> Result<String, String> {
    let api_base = api_base(&app_handle);

    let client = reqwest::Client::new();

    let mut url = format!("{}/api/agents/list?shop_id={}", api_base, branch_id);
    if let Some(z) = zone {
        url.push_str(&format!("&zone={}", z));
    }

    let resp = client
        .get(&url)
        .header("companycode", &company_code)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("List agents request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("List agents failed ({}): {}", status, text));
    }

    let list_resp: ListAgentsResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse list agents response: {}", e))?;

    serde_json::to_string(&list_resp.agents)
        .map_err(|e| format!("Failed to serialize agents: {}", e))
}

#[tauri::command]
pub async fn fetch_connected_agents(
    app_handle: tauri::AppHandle,
    company_code: String,
    shop_id: String,
) -> Result<String, String> {
    let api_base = api_base(&app_handle);

    let client = reqwest::Client::new();
    let url = format!("{}/api/agents/connected?shop_id={}", api_base, shop_id);

    let resp = client
        .get(&url)
        .header("companycode", &company_code)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Fetch connected agents request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Fetch connected agents failed ({}): {}", status, text));
    }

    let body_text = resp.text().await.map_err(|e| format!("Failed to read response body: {}", e))?;
    eprintln!("Connected agents response body: {}", body_text);

    let connected_resp: ConnectedAgentsResponse = serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse connected agents response: {} | Body was: {}", e, body_text))?;

    serde_json::to_string(&connected_resp)
        .map_err(|e| format!("Failed to serialize connected agents: {}", e))
}

#[tauri::command]
pub async fn auto_add_connected_agent(
    app_handle: tauri::AppHandle,
    _state: State<'_, Arc<AppState>>,
    agent_id: String,
    company_code: String,
    company_id: String,
    shop_id: String,
    zone: String,
    device_name: String,
) -> Result<(), String> {
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();

    let agent_exists = cfg.agents.iter().any(|a| a.agent_id == agent_id);
    if agent_exists {
        for agent in &mut cfg.agents {
            if agent.agent_id == agent_id {
                let needs_update = agent.branch_id.is_empty() || (agent.zone.is_empty() && !zone.is_empty());

                if agent.branch_id.is_empty() {
                    agent.branch_id = shop_id.clone();
                }
                if agent.zone.is_empty() && !zone.is_empty() {
                    agent.zone = zone.clone();
                }
                log::info!("Updated existing agent: {} with branch_id: {}, zone: {}", device_name, shop_id, zone);

                if needs_update {
                    sync_agent_to_api(&app_handle, agent, &company_code).await?;
                }
            }
        }
        config::save_config(&app_handle, &cfg)?;
        return Ok(());
    }

    let api_base = api_base(&app_handle);
    let client = reqwest::Client::new();
    let url = format!("{}/api/agents/{}", api_base, agent_id);

    let resp = client
        .get(&url)
        .header("companycode", &company_code)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await;

    let (printer_port, printer_type, printer_host, printer_network_port): (String, String, Option<String>, Option<i64>) = match resp {
        Ok(r) if r.status().is_success() => {
            match r.json::<ServerAgent>().await {
                Ok(agent_details) => (
                    agent_details.printer_port.unwrap_or_default(),
                    agent_details.printer_type.unwrap_or_else(|| "serial".to_string()),
                    agent_details.printer_host,
                    agent_details.printer_network_port,
                ),
                Err(_) => (String::new(), "serial".to_string(), None, None),
            }
        }
        _ => (String::new(), "serial".to_string(), None, None),
    };

    let new_agent = config::AgentConfig {
        agent_id: agent_id.clone(),
        company_code: company_code.clone(),
        company_id: company_id.clone(),
        branch_id: shop_id.clone(),
        zone: zone.clone(),
        device_name: device_name.clone(),
        device_token: String::new(),
        printer_port,
        paper_width: 80,
        ws_url: String::new(),
        printer_type,
        printer_host,
        printer_network_port,
        routing_mode: None,
        main_category_id: None,
        location: None,
        zones: Vec::new(),
        main_category_ids: Vec::new(),
        locations: Vec::new(),
        printer_language: None,
        cash_drawer: false,
        drawer_pin: None,
    };

    config::add_agent(&app_handle, new_agent.clone())?;
    log::info!("Auto-added connected agent: {} ({}) with branch_id: {}", device_name, agent_id, shop_id);

    Ok(())
}

async fn sync_agent_to_api(
    app_handle: &tauri::AppHandle,
    agent: &config::AgentConfig,
    company_code: &str,
) -> Result<(), String> {
    let api_base = api_base(app_handle);

    let client = reqwest::Client::new();
    let url = format!("{}/api/agents/{}/update", api_base, agent.agent_id);

    let mut payload = serde_json::json!({
        "printer_type": agent.printer_type,
        "printer_port": agent.printer_port,
    });
    // Never blank out server-side routing with empty local values.
    if !agent.branch_id.is_empty() {
        payload["branch_id"] = serde_json::Value::String(agent.branch_id.clone());
    }
    if !agent.zone.is_empty() {
        payload["zone"] = serde_json::Value::String(agent.zone.clone());
    }

    if let Some(ref host) = agent.printer_host {
        payload["printer_host"] = serde_json::Value::String(host.clone());
    }
    if let Some(port) = agent.printer_network_port {
        payload["printer_network_port"] = serde_json::Value::Number(port.into());
    }

    let resp = client
        .put(&url)
        .header("companycode", company_code)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Sync agent to API request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Sync agent to API failed ({}): {}", status, text));
    }

    log::info!("Synced agent {} to API with printer configuration", agent.agent_id);
    Ok(())
}

#[tauri::command]
pub async fn connect_to_existing_agent(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    company_code: String,
    agent_id: String,
    printer_port: String,
    printer_type: String,
    printer_host: Option<String>,
    printer_network_port: Option<i64>,
    shop_id: Option<String>,
    zone: Option<String>,
    device_name: Option<String>,
    company_id: Option<String>,
    routing_mode: Option<String>,
    zones: Option<Vec<String>>,
) -> Result<(), String> {
    // Reuse what we already know about this agent locally, then layer the
    // server-side details passed in from the setup screen on top.
    let existing = config::load_config(&app_handle)
        .and_then(|c| c.agents.into_iter().find(|a| a.agent_id == agent_id));
    let pick = |new: Option<String>, old: Option<&String>| {
        new.filter(|v| !v.is_empty())
            .or_else(|| old.cloned())
            .unwrap_or_default()
    };

    let zone = pick(zone, existing.as_ref().map(|a| &a.zone));
    let mut zones = zones.unwrap_or_default();
    if zones.is_empty() {
        zones = existing.as_ref().map(|a| a.zones.clone()).unwrap_or_default();
    }
    if zones.is_empty() && !zone.is_empty() {
        zones = vec![zone.clone()];
    }

    let new_agent = config::AgentConfig {
        agent_id: agent_id.clone(),
        company_code: company_code.clone(),
        company_id: pick(company_id, existing.as_ref().map(|a| &a.company_id)),
        branch_id: pick(shop_id, existing.as_ref().map(|a| &a.branch_id)),
        zone,
        device_name: pick(device_name, existing.as_ref().map(|a| &a.device_name)),
        device_token: existing.as_ref().map(|a| a.device_token.clone()).unwrap_or_default(),
        printer_port: printer_port.clone(),
        paper_width: 80,
        ws_url: existing.as_ref().map(|a| a.ws_url.clone()).unwrap_or_default(),
        printer_type: printer_type.clone(),
        printer_host: printer_host.clone(),
        printer_network_port,
        routing_mode: routing_mode
            .filter(|m| !m.is_empty())
            .or_else(|| existing.as_ref().and_then(|a| a.routing_mode.clone())),
        main_category_id: existing.as_ref().and_then(|a| a.main_category_id.clone()),
        location: existing.as_ref().and_then(|a| a.location.clone()),
        zones,
        main_category_ids: existing.as_ref().map(|a| a.main_category_ids.clone()).unwrap_or_default(),
        locations: existing.as_ref().map(|a| a.locations.clone()).unwrap_or_default(),
        printer_language: existing.as_ref().and_then(|a| a.printer_language.clone()),
        cash_drawer: existing.as_ref().map(|a| a.cash_drawer).unwrap_or(false),
        drawer_pin: existing.as_ref().and_then(|a| a.drawer_pin.clone()),
    };

    sync_agent_to_api(&app_handle, &new_agent, &company_code).await?;
    config::add_agent(&app_handle, new_agent.clone())?;

    let only_this = Config {
        agents: vec![new_agent],
        api_url: config::load_config(&app_handle).and_then(|c| c.api_url),
        ..Default::default()
    };
    start_ws_loop(state.inner().clone(), &only_this).await;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PendingJob {
    #[serde(rename = "_id")]
    pub _id: String,
    #[serde(rename = "job_id")]
    pub job_id: String,
    #[serde(rename = "shop_id")]
    pub shop_id: String,
    #[serde(rename = "zone")]
    pub zone: String,
    #[serde(rename = "main_category_id")]
    pub main_category_id: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub location_id: Option<String>,
    #[serde(default)]
    pub location_name: Option<String>,
    #[serde(default)]
    pub table_name: Option<String>,
    #[serde(rename = "order_no")]
    pub order_no: String,
    #[serde(rename = "content_type")]
    pub content_type: String,
    #[serde(rename = "cut_paper")]
    pub cut_paper: bool,
    #[serde(rename = "priority")]
    pub priority: String,
    #[serde(rename = "status")]
    pub status: String,
    pub lines: Vec<PrintJobLine>,
    #[serde(default, alias = "openDrawer")]
    pub open_drawer: bool,
    #[serde(default)]
    pub label: Option<printer::LabelJob>,
    #[serde(rename = "retry_count")]
    pub retry_count: i32,
    #[serde(rename = "created_at", default)]
    pub created_at: String,
}

#[tauri::command]
pub async fn fetch_pending_jobs(
    app_handle: tauri::AppHandle,
    shop_id: String,
    zone: String,
    status: Option<String>,
) -> Result<String, String> {
    let api_base = api_base(&app_handle);

    let client = reqwest::Client::new();

    let mut url = format!("{}/api/agents/pending-jobs?shop_id={}", api_base, shop_id);
    if !zone.is_empty() {
        url.push_str(&format!("&zone={}", urlencoding::encode(&zone)));
    }
    if let Some(s) = status {
        url.push_str(&format!("&status={}", s));
    }

    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Fetch pending jobs request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Fetch pending jobs failed ({}): {}", status, text));
    }

    let body = resp.text().await.map_err(|e| format!("Failed to read response body: {}", e))?;
    Ok(body)
}

/// Update an agent: pushes the change to the API first, then mirrors it into
/// the local config and restarts that agent's loops so it takes effect now.
/// `device_name` / `zone` are optional and only sent when provided.
#[tauri::command]
pub async fn update_agent_printer_port(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    printer_port: String,
    printer_type: String,
    printer_host: Option<String>,
    printer_network_port: Option<i64>,
    company_code: String,
    device_name: Option<String>,
    zone: Option<String>,
) -> Result<(), String> {
    let api_base = api_base(&app_handle);

    let client = reqwest::Client::new();
    let url = format!("{}/api/agents/{}/update", api_base, agent_id);

    let mut payload = serde_json::json!({
        "printer_port": printer_port,
        "printer_type": printer_type,
    });

    let device_name = device_name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    let zone = zone.map(|z| z.trim().to_string()).filter(|z| !z.is_empty());
    if let Some(ref name) = device_name {
        payload["device_name"] = serde_json::Value::String(name.clone());
    }
    if let Some(ref z) = zone {
        payload["zone"] = serde_json::Value::String(z.clone());
        payload["zones"] = serde_json::json!([z]);
    }

    if let Some(ref host) = printer_host {
        payload["printer_host"] = serde_json::Value::String(host.clone());
    }
    if let Some(port) = printer_network_port {
        payload["printer_network_port"] = serde_json::Value::Number(port.into());
    }

    let resp = client
        .put(&url)
        .header("companycode", &company_code)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Update agent request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Update agent failed ({}): {}", status, text));
    }

    let mut cfg = config::load_config(&app_handle).unwrap_or_default();
    if let Some(agent) = cfg.agents.iter_mut().find(|a| a.agent_id == agent_id) {
        agent.printer_port = printer_port.clone();
        agent.printer_type = printer_type.clone();
        agent.printer_host = printer_host.clone();
        agent.printer_network_port = printer_network_port;
        if let Some(name) = device_name {
            agent.device_name = name;
        }
        if let Some(z) = zone {
            agent.zone = z.clone();
            agent.zones = vec![z];
        }
        let updated_agent = agent.clone();
        config::save_config(&app_handle, &cfg)?;
        log::info!(
            "Updated local config for agent: {} - printer_type: {}, printer_port: {}, printer_host: {:?}, printer_network_port: {:?}",
            agent_id, printer_type, printer_port, printer_host, printer_network_port
        );
        // Restart this agent's WS/polling loops so the new printer and
        // routing are used immediately (running loops hold a config copy).
        let only_this = Config {
            agents: vec![updated_agent],
            api_url: cfg.api_url.clone(),
            ..Default::default()
        };
        start_ws_loop(state.inner().clone(), &only_this).await;
    }

    Ok(())
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

    let mut statuses = Vec::new();
    for agent in &cfg.agents {
        let ws_connected = connections
            .get(&agent.agent_id)
            .map(|c| c.connected)
            .unwrap_or(false);

        statuses.push(AgentStatus {
            agent_id: agent.agent_id.clone(),
            ws_connected,
            zone: agent.zone.clone(),
            branch_id: agent.branch_id.clone(),
            device_name: agent.device_name.clone(),
            queue_count: 0,
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
    printer_network_port: Option<i64>,
    routing_mode: Option<String>,
    main_category_id: Option<String>,
    location: Option<String>,
    zones: Option<Vec<String>>,
    main_category_ids: Option<Vec<String>>,
    locations: Option<Vec<String>>,
) -> Result<(), String> {
    let api_base = api_base(&app_handle);

    // The API returns a hardcoded WS URL; when pointing at a local/custom
    // API, derive the WS endpoint from the same host instead.
    let derived_ws_url = if api_base == LIVE_API_URL {
        None
    } else {
        Some(format!("{}/ws/agent", api_base.replacen("http", "ws", 1)))
    };

    let client = reqwest::Client::new();

    let cfg = config::load_config(&app_handle).unwrap_or_default();
    let existing_agent = cfg.agents.iter().find(|a| a.zone == zone && a.branch_id == branch_id);

    if let Some(agent) = existing_agent {
        log::info!("Found existing local agent for zone {}, updating printer configuration", zone);
        let updated_agent = config::AgentConfig {
            agent_id: agent.agent_id.clone(),
            company_id: agent.company_id.clone(),
            company_code: agent.company_code.clone(),
            branch_id: agent.branch_id.clone(),
            zone: agent.zone.clone(),
            device_name: device_name.clone(),
            device_token: agent.device_token.clone(),
            printer_port,
            paper_width: 80,
            ws_url: derived_ws_url.clone().unwrap_or_else(|| agent.ws_url.clone()),
            printer_type,
            printer_host,
            printer_network_port,
            routing_mode: routing_mode.clone(),
            main_category_id: main_category_id.clone(),
            location: location.clone(),
            zones: zones.clone().unwrap_or_default(),
            main_category_ids: main_category_ids.clone().unwrap_or_default(),
            locations: locations.clone().unwrap_or_default(),
            printer_language: agent.printer_language.clone(),
            cash_drawer: agent.cash_drawer,
            drawer_pin: agent.drawer_pin.clone(),
        };

        config::remove_agent(&app_handle, &agent.agent_id)?;
        config::add_agent(&app_handle, updated_agent)?;

        let new_cfg = config::load_config(&app_handle).unwrap_or_default();
        start_ws_loop(state.inner().clone(), &new_cfg).await;
        return Ok(());
    }

    let body = serde_json::json!({
        "companyCode": company_code,
        "branchId": branch_id,
        "zone": zone,
        "deviceName": device_name,
        "printerType": printer_type,
        "routingMode": routing_mode,
        "mainCategoryId": main_category_id,
        "location": location,
        "zones": zones,
        "mainCategoryIds": main_category_ids,
        "locations": locations,
    });

    log::info!("Pairing request to API: {} with body: {}", format!("{}/api/agents/pair", api_base), body);

    let resp = client
        .post(format!("{}/api/agents/pair", api_base))
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Pairing request failed: {}", e))?;

    let status = resp.status();
    log::info!("Pairing response status: {}", status);

    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        log::info!("Pairing error response body: {}", text);

        if status.as_u16() == 409 {
            log::info!("Agent already paired for zone {}, checking local config...", zone);
            let cfg = config::load_config(&app_handle).unwrap_or_default();
            let existing_agent = cfg.agents.iter().find(|a| a.zone == zone && a.branch_id == branch_id);

            if let Some(agent) = existing_agent {
                log::info!("Found existing agent for zone {}, reinitializing connection", zone);
                let updated_agent = config::AgentConfig {
                    agent_id: agent.agent_id.clone(),
                    company_id: agent.company_id.clone(),
                    company_code: agent.company_code.clone(),
                    branch_id: agent.branch_id.clone(),
                    zone: agent.zone.clone(),
                    device_name: device_name.clone(),
                    device_token: agent.device_token.clone(),
                    printer_port,
                    paper_width: 80,
                    ws_url: derived_ws_url.clone().unwrap_or_else(|| agent.ws_url.clone()),
                    printer_type,
                    printer_host,
                    printer_network_port,
                    routing_mode: routing_mode.clone(),
                    main_category_id: main_category_id.clone(),
                    location: location.clone(),
                    zones: zones.clone().unwrap_or_default(),
                    main_category_ids: main_category_ids.clone().unwrap_or_default(),
                    locations: locations.clone().unwrap_or_default(),
                    printer_language: agent.printer_language.clone(),
                    cash_drawer: agent.cash_drawer,
                    drawer_pin: agent.drawer_pin.clone(),
                };

                config::remove_agent(&app_handle, &agent.agent_id)?;
                config::add_agent(&app_handle, updated_agent)?;

                let new_cfg = config::load_config(&app_handle).unwrap_or_default();
                start_ws_loop(state.inner().clone(), &new_cfg).await;
                return Ok(());
            } else {
                // Another computer already owns this zone. Silently unpairing
                // it would take a working printer offline, so make the user
                // pick that agent under "Existing printers" instead.
                let server_msg = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                    .unwrap_or_else(|| format!("An agent is already paired for \"{}\"", zone));
                return Err(format!(
                    "{}. To print from this computer, choose it under \"Existing printers\" instead of adding a new one.",
                    server_msg
                ));
            }
        }

        log::error!("Pairing failed - Status: {}, Response: {}", status, text);
        return Err(format!("Pairing failed. HTTP Status: {}. API Response: {}", status, text));
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
        zone: zone.clone(),
        device_name,
        device_token: pair_resp.device_token,
        printer_port,
        paper_width: 80,
        ws_url: derived_ws_url.clone().unwrap_or(pair_resp.ws_url),
        printer_type,
        printer_host,
        printer_network_port,
        routing_mode: routing_mode.clone(),
        main_category_id: main_category_id.clone(),
        location: location.clone(),
        zones: zones.clone().unwrap_or_else(|| vec![zone.clone()]),
        main_category_ids: main_category_ids.clone().unwrap_or_default(),
        locations: locations.clone().unwrap_or_default(),
        printer_language: None,
        cash_drawer: false,
        drawer_pin: None,
    };

    config::add_agent(&app_handle, new_agent.clone())?;

    if let Err(e) = sync_agent_to_api(&app_handle, &new_agent, &company_code).await {
        log::error!("Failed to sync printer configuration to server: {}", e);
    }

    let cfg = config::load_config(&app_handle).unwrap_or_default();
    start_ws_loop(state.inner().clone(), &cfg).await;

    Ok(())
}

#[tauri::command]
pub async fn test_print(
    app_handle: tauri::AppHandle,
    agent_id: Option<String>,
    printer_type: Option<String>,
    printer_port: Option<String>,
    printer_host: Option<String>,
    printer_network_port: Option<i64>,
    printer_language: Option<String>,
) -> Result<(), String> {
    let cfg = config::load_config(&app_handle).unwrap_or_default();
    let explicit = printer_type.is_some()
        && (printer_port.as_deref().is_some_and(|p| !p.is_empty())
            || printer_host.as_deref().is_some_and(|h| !h.is_empty()));

    // Priority: the named agent, then explicit printer fields (setup / edit
    // screens testing an unsaved choice), then the first configured agent.
    let (target, lang) = if let Some(aid) = agent_id.filter(|a| !a.is_empty()) {
        let agent = devices::find_agent(&cfg, &aid)?;
        let target = devices::target_for(agent)?;
        let lang = match printer_language.as_deref() {
            Some(l) => printer::PrinterLanguage::parse(Some(l)),
            None => agent.language(),
        };
        (target, lang)
    } else if explicit || cfg.agents.is_empty() {
        let printer_type = printer_type.unwrap_or_else(|| "serial".to_string());
        let target = printer::PrintTarget::from_fields(
            &printer_type,
            printer_port.as_deref().unwrap_or(""),
            printer_host.as_deref(),
            printer_network_port.or(Some(9100)),
        );
        (target, printer::PrinterLanguage::parse(printer_language.as_deref()))
    } else {
        let agent = &cfg.agents[0];
        (devices::target_for(agent)?, agent.language())
    };

    tokio::task::spawn_blocking(move || printer::test_print_as(&target, lang))
        .await
        .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Devices: cash drawer, printer status, labels, scales, local API
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn open_cash_drawer(
    state: State<'_, Arc<AppState>>,
    agent_id: Option<String>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || devices::open_drawer_blocking(&state, agent_id.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

/// Status of one printer; `refresh` queries the device now instead of
/// returning the background monitor's last result.
#[tauri::command]
pub async fn get_printer_status(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    refresh: Option<bool>,
) -> Result<devices::StatusEntry, String> {
    if !refresh.unwrap_or(false) {
        if let Some(entry) = devices::cached_statuses(&state).remove(&agent_id) {
            return Ok(entry);
        }
    }
    devices::refresh_status(state.inner().clone(), agent_id).await
}

#[tauri::command]
pub fn get_printer_statuses(
    state: State<'_, Arc<AppState>>,
) -> std::collections::HashMap<String, devices::StatusEntry> {
    devices::cached_statuses(&state)
}

/// Local-only device settings for a printer agent. Restarts the agent's
/// loops so the WebSocket re-advertises its capabilities.
#[tauri::command]
pub async fn set_agent_device_options(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    printer_language: Option<String>,
    cash_drawer: Option<bool>,
    drawer_pin: Option<String>,
) -> Result<(), String> {
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();
    let agent = cfg
        .agents
        .iter_mut()
        .find(|a| a.agent_id == agent_id)
        .ok_or_else(|| format!("Agent {} not found", agent_id))?;
    if let Some(lang) = printer_language {
        agent.printer_language = Some(printer::PrinterLanguage::parse(Some(&lang)).as_str().to_string());
    }
    if let Some(d) = cash_drawer {
        agent.cash_drawer = d;
    }
    if let Some(pin) = drawer_pin.filter(|p| !p.is_empty()) {
        agent.drawer_pin = Some(pin);
    }
    let only_this = Config {
        agents: vec![agent.clone()],
        api_url: cfg.api_url.clone(),
        ..Default::default()
    };
    config::save_config(&app_handle, &cfg)?;
    start_ws_loop(state.inner().clone(), &only_this).await;
    Ok(())
}

#[tauri::command]
pub async fn print_test_label(
    state: State<'_, Arc<AppState>>,
    agent_id: Option<String>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        devices::print_label_blocking(&state, agent_id.as_deref(), &printer::LabelJob::test_label("Test label"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn list_scales(app_handle: tauri::AppHandle) -> Vec<printer::ScaleConfig> {
    config::load_config(&app_handle).unwrap_or_default().scales
}

/// Insert or update a scale (an empty id creates a new one). Returns its id.
#[tauri::command]
pub async fn save_scale(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    mut scale: printer::ScaleConfig,
) -> Result<String, String> {
    if scale.name.trim().is_empty() {
        return Err("Give the scale a name".into());
    }
    if scale.id.trim().is_empty() {
        scale.id = format!("scale_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    }
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();
    match cfg.scales.iter_mut().find(|s| s.id == scale.id) {
        Some(existing) => *existing = scale.clone(),
        None => cfg.scales.push(scale.clone()),
    }
    config::save_config(&app_handle, &cfg)?;
    // Re-advertise capabilities (scale list) to the server.
    start_ws_loop(state.inner().clone(), &cfg).await;
    Ok(scale.id)
}

#[tauri::command]
pub async fn remove_scale(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    scale_id: String,
) -> Result<(), String> {
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();
    cfg.scales.retain(|s| s.id != scale_id);
    config::save_config(&app_handle, &cfg)?;
    start_ws_loop(state.inner().clone(), &cfg).await;
    Ok(())
}

/// Read a weight from a saved scale (`scale_id`) or an unsaved config
/// (`scale`, used by the "Test" button while setting one up).
#[tauri::command]
pub async fn read_scale(
    app_handle: tauri::AppHandle,
    scale_id: Option<String>,
    scale: Option<printer::ScaleConfig>,
) -> Result<printer::WeightReading, String> {
    let scale = match scale {
        Some(s) => s,
        None => {
            let cfg = config::load_config(&app_handle).unwrap_or_default();
            devices::find_scale(&cfg, scale_id.as_deref())?.clone()
        }
    };
    tokio::task::spawn_blocking(move || printer::scale::read_weight(&scale))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn scale_command(
    app_handle: tauri::AppHandle,
    scale_id: Option<String>,
    command: String,
) -> Result<(), String> {
    let cfg = config::load_config(&app_handle).unwrap_or_default();
    let scale = devices::find_scale(&cfg, scale_id.as_deref())?.clone();
    tokio::task::spawn_blocking(move || printer::scale::send_command(&scale, &command))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_local_api_info(app_handle: tauri::AppHandle, state: State<'_, Arc<AppState>>) -> serde_json::Value {
    let cfg = config::load_config(&app_handle).unwrap_or_default().local_api;
    let mut info = state.local_api_info.lock().map(|i| i.clone()).unwrap_or_default();
    info["enabled"] = serde_json::json!(cfg.enabled);
    info["port"] = serde_json::json!(cfg.port);
    info["allowedOrigins"] = serde_json::json!(cfg.allowed_origins);
    info
}

#[tauri::command]
pub fn set_local_api_config(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    enabled: bool,
    port: Option<u16>,
    allowed_origins: Option<Vec<String>>,
) -> Result<(), String> {
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();
    cfg.local_api.enabled = enabled;
    if let Some(p) = port.filter(|p| *p >= 1024) {
        cfg.local_api.port = p;
    }
    if let Some(origins) = allowed_origins {
        cfg.local_api.allowed_origins = origins
            .into_iter()
            .map(|o| o.trim().trim_end_matches('/').to_string())
            .filter(|o| !o.is_empty())
            .collect();
    }
    config::save_config(&app_handle, &cfg)?;
    crate::local_api::start(state.inner().clone());
    Ok(())
}

/// Deactivate the agent on the API. A 404 counts as success (already gone).
async fn unpair_on_api(
    app_handle: &tauri::AppHandle,
    agent_id: &str,
    company_code: &str,
) -> Result<(), String> {
    let url = format!("{}/api/agents/{}/unpair", api_base(app_handle), agent_id);
    let resp = reqwest::Client::new()
        .delete(&url)
        .header("companycode", company_code)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    let status = resp.status();
    if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }
    let text = resp.text().await.unwrap_or_default();
    Err(format!("{}: {}", status, text))
}

/// Removes the agent from the API and from this machine. The local removal
/// always happens; if the API call fails, a warning is returned so the UI can
/// tell the user the server-side record may still exist.
#[tauri::command]
pub async fn unpair_device(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<Option<String>, String> {
    log::info!("Unpairing agent: {}", agent_id);

    let company_code = config::load_config(&app_handle)
        .and_then(|c| c.agents.into_iter().find(|a| a.agent_id == agent_id))
        .map(|a| a.company_code)
        .unwrap_or_default();

    let warning = match unpair_on_api(&app_handle, &agent_id, &company_code).await {
        Ok(()) => None,
        Err(e) => {
            log::warn!("API unpair failed for {}: {}", agent_id, e);
            Some(format!("Removed locally, but the server could not be updated ({})", e))
        }
    };

    // Stop this agent's WS/polling loops before dropping its config.
    state.stop_agent_tasks(&agent_id);
    config::remove_agent(&app_handle, &agent_id)?;

    let mut connections = state.agent_connections.lock().await;
    connections.remove(&agent_id);
    log::info!("Removed agent connection for: {}", agent_id);
    drop(connections);

    if let Ok(mut statuses) = state.printer_status.lock() {
        statuses.remove(&agent_id);
    }

    let cfg = config::load_config(&app_handle).unwrap_or_default();
    if cfg.agents.is_empty() {
        log::info!("No agents remaining, deleting config file");
        config::delete_config_if_unused(&app_handle)?;
    }

    Ok(warning)
}

#[tauri::command]
pub async fn unpair_all_agents(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let cfg = config::load_config(&app_handle).unwrap_or_default();
    let agent_ids: Vec<String> = cfg.agents.iter().map(|a| a.agent_id.clone()).collect();

    if agent_ids.is_empty() {
        return Ok("No agents to unpair".to_string());
    }

    log::info!("Unpairing all agents: {:?}", agent_ids);

    let mut api_failures = 0usize;
    for agent in &cfg.agents {
        if let Err(e) = unpair_on_api(&app_handle, &agent.agent_id, &agent.company_code).await {
            log::warn!("API unpair failed for {}: {}", agent.agent_id, e);
            api_failures += 1;
        }
    }

    for agent_id in &agent_ids {
        state.stop_agent_tasks(agent_id);
        if let Err(e) = config::remove_agent(&app_handle, agent_id) {
            log::error!("Failed to remove agent {}: {}", agent_id, e);
        }
    }

    let mut connections = state.agent_connections.lock().await;
    connections.clear();
    drop(connections);
    if let Ok(mut statuses) = state.printer_status.lock() {
        statuses.clear();
    }

    config::delete_config_if_unused(&app_handle)?;

    log::info!("All agents unpaired successfully");
    if api_failures > 0 {
        return Ok(format!(
            "Unpaired {} agent(s) locally; {} could not be removed on the server",
            agent_ids.len(),
            api_failures
        ));
    }
    Ok(format!("Unpaired {} agent(s)", agent_ids.len()))
}

#[tauri::command]
pub async fn test_printer_connection(
    printer_host: String,
    printer_network_port: u16,
) -> Result<String, String> {
    use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
    use std::time::Duration;

    let address_str = format!("{}:{}", printer_host, printer_network_port);
    log::info!("Testing printer connection to {}", address_str);

    let socket_addr: SocketAddr = match address_str.parse() {
        Ok(addr) => addr,
        Err(_e) => match (printer_host.as_str(), printer_network_port).to_socket_addrs() {
            Ok(mut addrs) => match addrs.next() {
                Some(addr) => addr,
                None => return Err(format!("No addresses found for {}", printer_host)),
            },
            Err(e) => return Err(format!("Failed to resolve address: {}", e)),
        },
    };

    match TcpStream::connect_timeout(&socket_addr, Duration::from_secs(5)) {
        Ok(_) => {
            log::info!("Successfully connected to printer at {}", socket_addr);
            Ok(format!("Successfully connected to printer at {}", socket_addr))
        }
        Err(e) => {
            log::error!("Failed to connect to printer at {}: {}", socket_addr, e);
            Err(format!("Failed to connect to printer at {}: {}", socket_addr, e))
        }
    }
}

#[tauri::command]
pub async fn fetch_agents_from_api(app_handle: tauri::AppHandle, shop_id: String, company_code: String) -> Result<Vec<ServerAgent>, String> {
    let api_base = api_base(&app_handle);
    let client = reqwest::Client::new();
    let url = format!("{}/api/agents/list?shop_id={}", api_base, shop_id);

    log::info!("Fetching agents from API: {}", url);
    log::info!("Using company code header: {}", company_code);

    let response = client
        .get(&url)
        .header("companycode", &company_code)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    match response {
        Ok(resp) => {
            log::info!("API response status: {}", resp.status());
            let body = resp.text().await;
            match &body {
                Ok(text) => log::info!("API response body: {}", text),
                Err(e) => log::error!("Failed to read response body: {}", e),
            }

            #[derive(serde::Deserialize)]
            struct AgentsResponse {
                agents: Vec<ServerAgent>,
            }

            let body_str = body.map_err(|e| format!("Failed to read response: {}", e))?;
            let agents_response: AgentsResponse = serde_json::from_str(&body_str)
                .map_err(|e| format!("Failed to parse agents response: {} (body: {})", e, body_str))?;

            log::info!("Fetched {} agents from API", agents_response.agents.len());
            Ok(agents_response.agents)
        }
        Err(e) => {
            log::error!("Failed to fetch agents from API: {}", e);
            Err(format!("Failed to fetch agents from API: {}", e))
        }
    }
}

#[tauri::command]
pub async fn remove_duplicate_agents(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<usize, String> {
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();

    log::info!("Current agent count: {}", cfg.agents.len());

    let mut seen_ids = std::collections::HashSet::new();
    let mut duplicates_to_remove = Vec::new();

    for (index, agent) in cfg.agents.iter().enumerate() {
        if seen_ids.contains(&agent.agent_id) {
            duplicates_to_remove.push(index);
        } else {
            seen_ids.insert(agent.agent_id.clone());
        }
    }

    let count = duplicates_to_remove.len();
    log::info!("Found {} duplicates to remove", count);

    for index in duplicates_to_remove.into_iter().rev() {
        let removed = cfg.agents.remove(index);
        log::info!("Removed duplicate agent: {:?}", removed.device_name);
    }

    config::save_config(&app_handle, &cfg)?;
    start_ws_loop(state.inner().clone(), &cfg).await;

    Ok(count)
}