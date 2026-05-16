use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

fn default_printer_type() -> String {
    "serial".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub agent_id: String,
    pub company_id: String,
    pub company_code: String,
    pub branch_id: String,
    pub zone: String,
    pub device_name: String,
    pub device_token: String,
    pub printer_port: String,
    pub paper_width: u32,
    pub ws_url: String,
    #[serde(default = "default_printer_type")]
    pub printer_type: String,
    #[serde(default)]
    pub printer_host: Option<String>,
    #[serde(default)]
    pub printer_network_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub agents: Vec<AgentConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            agents: Vec::new(),
        }
    }
}

fn config_path(app_handle: &tauri::AppHandle) -> PathBuf {
    app_handle
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir")
        .join("config.json")
}

pub fn load_config(app_handle: &tauri::AppHandle) -> Option<Config> {
    let path = config_path(app_handle);
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &Config) -> Result<(), String> {
    let path = config_path(app_handle);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_config(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let path = config_path(app_handle);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn add_agent(app_handle: &tauri::AppHandle, agent: AgentConfig) -> Result<(), String> {
    let mut config = load_config(app_handle).unwrap_or_default();
    config.agents.push(agent);
    save_config(app_handle, &config)
}

pub fn remove_agent(app_handle: &tauri::AppHandle, agent_id: &str) -> Result<(), String> {
    let mut config = load_config(app_handle).unwrap_or_default();
    config.agents.retain(|a| a.agent_id != agent_id);
    save_config(app_handle, &config)
}
