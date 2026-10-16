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
    pub printer_network_port: Option<i64>,
    /// "category" (default) routes jobs by main category / zone;
    /// "location" routes jobs by dining location/section.
    #[serde(default)]
    pub routing_mode: Option<String>,
    #[serde(default)]
    pub main_category_id: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    /// Multiple routing targets (one agent can serve several categories or
    /// locations). `zone`/`main_category_id`/`location` remain as the
    /// single-value fields for backwards compatibility.
    #[serde(default)]
    pub zones: Vec<String>,
    #[serde(default)]
    pub main_category_ids: Vec<String>,
    #[serde(default)]
    pub locations: Vec<String>,
}

impl AgentConfig {
    /// All routing values this agent accepts, singular + plural combined.
    pub fn all_zones(&self) -> Vec<String> {
        let mut v = self.zones.clone();
        if !self.zone.is_empty() && !v.contains(&self.zone) {
            v.push(self.zone.clone());
        }
        v
    }

    pub fn all_main_category_ids(&self) -> Vec<String> {
        let mut v = self.main_category_ids.clone();
        if let Some(id) = &self.main_category_id {
            if !id.is_empty() && !v.contains(id) {
                v.push(id.clone());
            }
        }
        v
    }

    pub fn all_locations(&self) -> Vec<String> {
        let mut v = self.locations.clone();
        if let Some(loc) = &self.location {
            if !loc.is_empty() && !v.contains(loc) {
                v.push(loc.clone());
            }
        }
        v
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub agents: Vec<AgentConfig>,
    /// Optional API base URL override (e.g. http://localhost:3002 for local
    /// development). Falls back to the live API when unset.
    #[serde(default)]
    pub api_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            agents: Vec::new(),
            api_url: None,
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
    let mut cfg: Config = serde_json::from_str(&content).ok()?;
    dedupe_agents(&mut cfg);
    Some(cfg)
}

/// Keep only the last entry for each agent_id (the most recently written one).
fn dedupe_agents(cfg: &mut Config) {
    let mut seen = std::collections::HashSet::new();
    let mut kept: Vec<AgentConfig> = Vec::with_capacity(cfg.agents.len());
    for agent in cfg.agents.drain(..).rev() {
        if seen.insert(agent.agent_id.clone()) {
            kept.push(agent);
        }
    }
    kept.reverse();
    cfg.agents = kept;
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

/// Insert the agent, or replace the existing entry with the same agent_id
/// (never creates duplicates).
pub fn add_agent(app_handle: &tauri::AppHandle, agent: AgentConfig) -> Result<(), String> {
    let mut config = load_config(app_handle).unwrap_or_default();
    match config.agents.iter_mut().find(|a| a.agent_id == agent.agent_id) {
        Some(existing) => *existing = agent,
        None => config.agents.push(agent),
    }
    save_config(app_handle, &config)
}

pub fn remove_agent(app_handle: &tauri::AppHandle, agent_id: &str) -> Result<(), String> {
    let mut config = load_config(app_handle).unwrap_or_default();
    config.agents.retain(|a| a.agent_id != agent_id);
    save_config(app_handle, &config)
}
