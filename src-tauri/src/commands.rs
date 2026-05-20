use crate::config::{self, Config};
use crate::printer;
use crate::AppState;
use crate::start_ws_loop;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobLine {
    #[serde(rename = "type")]
    pub line_type: String,
    pub text: String,
    pub qty: Option<u32>,
    pub price: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    #[serde(rename = "_id")]
    pub id: Option<String>,
    pub job_id: String,
    pub branch_id: String,
    pub destination_zone: String,
    pub content_type: String,
    pub lines: Vec<PrintJobLine>,
    pub cut_paper: bool,
    pub priority: String,
    pub created_at: String,
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
pub async fn discover_network_printers() -> Vec<String> {
    printer::discover_network_printers().await
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Shop {
    pub _id: String,
    pub name: String,
    #[serde(rename = "shop_code")]
    pub shop_code: Option<String>,
}

#[tauri::command]
pub async fn fetch_shops(company_code: String) -> Result<String, String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());

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

#[tauri::command]
pub async fn list_agents(
    company_code: String,
    branch_id: String,
    zone: Option<String>,
) -> Result<String, String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());

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
    company_code: String,
    shop_id: String,
) -> Result<String, String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());

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

    // Check if agent already exists in local config and update it if it does
    let agent_exists = cfg.agents.iter().any(|a| a.agent_id == agent_id);
    if agent_exists {
        // Update existing agent with shop_id and zone if they're missing
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

                // Sync to API if fields were updated
                if needs_update {
                    sync_agent_to_api(&app_handle, agent, &company_code).await?;
                }
            }
        }
        config::save_config(&app_handle, &cfg)?;
        return Ok(());
    }

    // Fetch agent details from API to get printer configuration
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());
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
                Err(_) => (
                    String::new(),
                    "serial".to_string(),
                    None,
                    None,
                ),
            }
        }
        _ => (
            String::new(),
            "serial".to_string(),
            None,
            None,
        ),
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
    };

    config::add_agent(&app_handle, new_agent.clone())?;
    log::info!("Auto-added connected agent: {} ({}) with branch_id: {}", device_name, agent_id, shop_id);

    // Don't start WebSocket loop here - agent doesn't have credentials yet
    Ok(())
}

async fn sync_agent_to_api(
    _app_handle: &tauri::AppHandle,
    agent: &config::AgentConfig,
    company_code: &str,
) -> Result<(), String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());

    let client = reqwest::Client::new();

    let url = format!("{}/api/agents/{}/update", api_base, agent.agent_id);

    let mut payload = serde_json::json!({
        "branch_id": agent.branch_id,
        "zone": agent.zone,
        "printer_type": agent.printer_type,
        "printer_port": agent.printer_port,
    });

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
) -> Result<(), String> {
    let cfg = config::load_config(&app_handle).unwrap_or_default();

    let new_agent = config::AgentConfig {
        agent_id: agent_id.clone(),
        company_code: company_code.clone(),
        company_id: String::new(), // Will be fetched from API
        branch_id: String::new(),
        zone: String::new(), // Will be fetched from API
        device_name: String::new(), // Will be fetched from API
        device_token: String::new(),
        printer_port: printer_port.clone(),
        paper_width: 80,
        ws_url: String::new(),
        printer_type: printer_type.clone(),
        printer_host: printer_host.clone(),
        printer_network_port,
    };

    config::add_agent(&app_handle, new_agent.clone())?;

    // Sync printer configuration to API
    sync_agent_to_api(&app_handle, &new_agent, &company_code).await?;

    start_ws_loop(state.inner().clone(), &cfg).await;
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
    #[serde(rename = "retry_count")]
    pub retry_count: i32,
    #[serde(rename = "created_at", default)]
    pub created_at: String,
}

#[tauri::command]
pub async fn fetch_pending_jobs(
    shop_id: String,
    zone: String,
    status: Option<String>,
) -> Result<String, String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());

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

#[tauri::command]
pub async fn update_agent_printer_port(
    app_handle: tauri::AppHandle,
    agent_id: String,
    printer_port: String,
    printer_type: String,
    printer_host: Option<String>,
    printer_network_port: Option<i64>,
    company_code: String,
) -> Result<(), String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());

    let client = reqwest::Client::new();

    let url = format!("{}/api/agents/{}/update", api_base, agent_id);

    let mut payload = serde_json::json!({
        "printer_port": printer_port,
        "printer_type": printer_type,
    });

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

    // Update local config
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();
    if let Some(agent) = cfg.agents.iter_mut().find(|a| a.agent_id == agent_id) {
        agent.printer_port = printer_port.clone();
        agent.printer_type = printer_type.clone();
        agent.printer_host = printer_host.clone();
        agent.printer_network_port = printer_network_port;
        config::save_config(&app_handle, &cfg)?;
        log::info!("Updated local config for agent: {} - printer_type: {}, printer_port: {}, printer_host: {:?}, printer_network_port: {:?}",
            agent_id, printer_type, printer_port, printer_host, printer_network_port);
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

        // Queue count is now tracked by the API, local tracking removed
        let queue_count = 0;

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
    printer_network_port: Option<i64>,
) -> Result<(), String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());

    let client = reqwest::Client::new();

    // Check if we already have an agent for this zone locally
    let cfg = config::load_config(&app_handle).unwrap_or_default();
    let existing_agent = cfg.agents.iter().find(|a| a.zone == zone && a.branch_id == branch_id);

    if let Some(agent) = existing_agent {
        log::info!("Found existing local agent for zone {}, updating printer configuration", zone);
        // Update the existing agent's configuration with new printer settings
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
            ws_url: agent.ws_url.clone(),
            printer_type,
            printer_host,
            printer_network_port,
        };

        // Update the config by removing the old agent and adding the updated one
        config::remove_agent(&app_handle, &agent.agent_id)?;
        config::add_agent(&app_handle, updated_agent)?;

        log::info!("Agent printer configuration updated, restarting WebSocket loop for all agents...");
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
        
        // Handle 409 Conflict - agent already paired for this zone
        if status.as_u16() == 409 {
            log::info!("Agent already paired for zone {}, checking local config...", zone);
            let cfg = config::load_config(&app_handle).unwrap_or_default();
            
            // Check if we have an agent for this zone locally
            let existing_agent = cfg.agents.iter().find(|a| a.zone == zone && a.branch_id == branch_id);
            
            if let Some(agent) = existing_agent {
                log::info!("Found existing agent for zone {}, reinitializing connection", zone);
                // Update the existing agent's configuration with new printer settings
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
                    ws_url: agent.ws_url.clone(),
                    printer_type,
                    printer_host,
                    printer_network_port,
                };
                
                // Update the config by removing the old agent and adding the updated one
                config::remove_agent(&app_handle, &agent.agent_id)?;
                config::add_agent(&app_handle, updated_agent)?;
                
                log::info!("Agent updated, restarting WebSocket loop for all agents...");
                let new_cfg = config::load_config(&app_handle).unwrap_or_default();
                start_ws_loop(state.inner().clone(), &new_cfg).await;
                
                return Ok(());
            } else {
                // No local agent for this zone, but API says one exists
                // Try to unpair the existing agent from the server
                log::info!("No local agent found, attempting to list and unpair existing agent from server...");
                
                let list_resp = list_agents(company_code.clone(), branch_id.clone(), Some(zone.clone())).await;
                
                match list_resp {
                    Ok(agents_json) => {
                        let agents: Vec<ServerAgent> = serde_json::from_str(&agents_json)
                            .map_err(|e| format!("Failed to parse agents JSON: {}", e))?;
                        
                        if let Some(existing_agent) = agents.first() {
                            log::info!("Found existing agent on server: {}, attempting to unpair", existing_agent.agent_id);
                            
                            // Call unpair endpoint
                            let unpair_url = format!("{}/api/agents/{}/unpair", api_base, existing_agent.agent_id);
                            let unpair_resp = client
                                .post(&unpair_url)
                                .header("companycode", &company_code)
                                .timeout(std::time::Duration::from_secs(15))
                                .send()
                                .await;
                            
                            match unpair_resp {
                                Ok(resp) if resp.status().is_success() => {
                                    log::info!("Successfully unpaired existing agent from server, retrying pair...");
                                    // Retry the pairing request
                                    let retry_resp = client
                                        .post(format!("{}/api/agents/pair", api_base))
                                        .json(&body)
                                        .timeout(std::time::Duration::from_secs(15))
                                        .send()
                                        .await
                                        .map_err(|e| format!("Retry pairing request failed: {}", e))?;
                                    
                                    if !retry_resp.status().is_success() {
                                        let status = retry_resp.status();
                                        let text = retry_resp.text().await.unwrap_or_default();
                                        return Err(format!("Retry pairing failed ({}): {}", status, text));
                                    }
                                    
                                    let pair_resp: PairResponse = retry_resp
                                        .json()
                                        .await
                                        .map_err(|e| format!("Failed to parse retry pairing response: {}", e))?;
                                    
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
                                    
                                    log::info!("Agent added after unpair, starting WebSocket loop for all agents...");
                                    let cfg = config::load_config(&app_handle).unwrap_or_default();
                                    start_ws_loop(state.inner().clone(), &cfg).await;
                                    
                                    return Ok(());
                                }
                                Ok(resp) => {
                                    let status = resp.status();
                                    let text = resp.text().await.unwrap_or_default();
                                    log::error!("Failed to unpair existing agent: {} - {}", status, text);
                                    return Err(format!("Failed to unpair existing agent from server ({}): {}. Please remove it manually from the server admin panel.", status, text));
                                }
                                Err(e) => {
                                    log::error!("Failed to unpair existing agent: {}", e);
                                    return Err(format!("Failed to unpair existing agent from server: {}. Please remove it manually from the server admin panel.", e));
                                }
                            }
                        } else {
                            return Err(format!("Agent already paired for this zone on the server, but no agents found in list. Error: {}", text));
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to list agents: {}", e);
                        return Err(format!("Failed to list agents from server: {}. Please remove the existing agent manually from the server admin panel. Original error: {}", e, text));
                    }
                }
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

    config::add_agent(&app_handle, new_agent.clone())?;

    log::info!("Agent added, syncing printer configuration to server...");
    if let Err(e) = sync_agent_to_api(&app_handle, &new_agent, &company_code).await {
        log::error!("Failed to sync printer configuration to server: {}", e);
    }

    log::info!("Agent added, starting WebSocket loop for all agents...");
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
                port: printer_network_port as u16,
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
    log::info!("Unpairing agent: {}", agent_id);
    
    config::remove_agent(&app_handle, &agent_id)?;

    let mut connections = state.agent_connections.lock().await;
    connections.remove(&agent_id);
    log::info!("Removed agent connection for: {}", agent_id);
    drop(connections);

    let cfg = config::load_config(&app_handle).unwrap_or_default();
    if cfg.agents.is_empty() {
        log::info!("No agents remaining, deleting config file");
        config::delete_config(&app_handle)?;
    } else {
        log::info!("Restarting WebSocket loop for remaining agents");
        start_ws_loop(state.inner().clone(), &cfg).await;
    }

    Ok(())
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

    for agent_id in &agent_ids {
        if let Err(e) = config::remove_agent(&app_handle, agent_id) {
            log::error!("Failed to remove agent {}: {}", agent_id, e);
        }
    }

    // Clear WebSocket connections
    let mut connections = state.agent_connections.lock().await;
    connections.clear();
    drop(connections);

    // Delete config file
    config::delete_config(&app_handle)?;

    log::info!("All agents unpaired successfully");
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

    // Parse the address string to SocketAddr
    let socket_addr: SocketAddr = match address_str.parse() {
        Ok(addr) => addr,
        Err(_e) => {
            // Try to resolve as hostname
            match (printer_host.as_str(), printer_network_port).to_socket_addrs() {
                Ok(mut addrs) => {
                    match addrs.next() {
                        Some(addr) => addr,
                        None => return Err(format!("No addresses found for {}", printer_host)),
                    }
                }
                Err(e) => {
                    return Err(format!("Failed to resolve address: {}", e));
                }
            }
        }
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
pub async fn fetch_agents_from_api(shop_id: String, company_code: String) -> Result<Vec<ServerAgent>, String> {
    let api_base = std::env::var("PRINT_AGENT_API_URL")
        .unwrap_or_else(|_| "https://api.hospitality.reliatech.co.ke".to_string());
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
pub async fn remove_duplicate_agents(app_handle: tauri::AppHandle, state: State<'_, Arc<AppState>>) -> Result<usize, String> {
    let mut cfg = config::load_config(&app_handle).unwrap_or_default();

    log::info!("Current agent count: {}", cfg.agents.len());
    for (i, agent) in cfg.agents.iter().enumerate() {
        log::info!("Agent {}: {} (zone: {})", i, agent.agent_id, agent.zone);
    }

    // Track seen agent IDs to identify duplicates
    let mut seen_ids = std::collections::HashSet::new();
    let mut duplicates_to_remove = Vec::new();

    for (index, agent) in cfg.agents.iter().enumerate() {
        if seen_ids.contains(&agent.agent_id) {
            log::info!("Found duplicate at index {}: {}", index, agent.agent_id);
            duplicates_to_remove.push(index);
        } else {
            seen_ids.insert(agent.agent_id.clone());
        }
    }

    let count = duplicates_to_remove.len();
    log::info!("Found {} duplicates to remove", count);

    // Remove duplicates in reverse order to maintain indices
    for index in duplicates_to_remove.into_iter().rev() {
        let removed_agent = cfg.agents.remove(index);
        log::info!("Removed duplicate agent at index {}: {:?}", index, removed_agent.device_name);
    }

    config::save_config(&app_handle, &cfg)?;
    log::info!("Saved config with {} agents", cfg.agents.len());

    // Restart WebSocket loop with cleaned config
    start_ws_loop(state.inner().clone(), &cfg).await;

    Ok(count)
}
