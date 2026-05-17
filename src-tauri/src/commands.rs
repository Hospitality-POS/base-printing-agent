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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAgent {
    pub agent_id: String,
    pub shop_id: String,
    pub zone: String,
    pub device_name: String,
    pub is_active: bool,
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

    let url = format!("{}/shops", api_base);

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
