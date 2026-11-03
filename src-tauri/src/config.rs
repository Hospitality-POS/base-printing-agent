use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
    /// "escpos" (receipt, default), "zpl" or "tspl" (label printers).
    #[serde(default)]
    pub printer_language: Option<String>,
    /// A cash drawer is wired to this printer's drawer-kick port.
    #[serde(default)]
    pub cash_drawer: bool,
    /// Drawer-kick pin: "2" (default), "5" or "both".
    #[serde(default)]
    pub drawer_pin: Option<String>,
}

impl AgentConfig {
    pub fn language(&self) -> printer_core::PrinterLanguage {
        printer_core::PrinterLanguage::parse(self.printer_language.as_deref())
    }

    pub fn drawer_pin(&self) -> printer_core::DrawerPin {
        printer_core::DrawerPin::parse(self.drawer_pin.as_deref())
    }

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
    /// Weighing scales attached to this computer.
    #[serde(default)]
    pub scales: Vec<printer_core::ScaleConfig>,
    #[serde(default)]
    pub local_api: LocalApiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            agents: Vec::new(),
            api_url: None,
            scales: Vec::new(),
            local_api: LocalApiConfig::default(),
        }
    }
}

pub const DEFAULT_LOCAL_API_PORT: u16 = 14210;

/// Localhost HTTP API the POS web app calls for low-latency device access
/// (scale weight, cash drawer, printer status, direct printing).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalApiConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_local_api_port")]
    pub port: u16,
    /// Browser origins allowed to call the API; `*` wildcards allowed.
    /// Requests without an Origin header (native apps) are always allowed.
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_local_api_port() -> u16 {
    DEFAULT_LOCAL_API_PORT
}

pub fn default_allowed_origins() -> Vec<String> {
    printer_core::origin::DEFAULT_ALLOWED_ORIGINS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Default for LocalApiConfig {
    fn default() -> Self {
        LocalApiConfig {
            enabled: true,
            port: DEFAULT_LOCAL_API_PORT,
            allowed_origins: default_allowed_origins(),
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

/// Copy of the last config that parsed, kept next to it by every save.
fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

/// Serialises read-modify-write of the config file (see `update`), so two
/// commands saving at once can't drop each other's changes.
static CONFIG_LOCK: Mutex<()> = Mutex::new(());

fn read_file(path: &Path) -> Result<Option<Config>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut cfg: Config = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    dedupe_agents(&mut cfg);
    Ok(Some(cfg))
}

pub fn load_config(app_handle: &tauri::AppHandle) -> Option<Config> {
    load_from(&config_path(app_handle))
}

fn load_from(path: &Path) -> Option<Config> {
    match read_file(path) {
        Ok(cfg) => cfg,
        // A damaged file must not read as "no printers" — callers would then
        // save an empty config over it. Use the last good copy instead.
        Err(e) => {
            log::error!("Config {} is unreadable ({}); using the backup", path.display(), e);
            match read_file(&backup_path(path)) {
                Ok(cfg) => cfg,
                Err(e) => {
                    log::error!("Config backup is unreadable too: {}", e);
                    None
                }
            }
        }
    }
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
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    write_to(&config_path(app_handle), config)
}

/// Load, change and save the config as one step, holding the config lock so
/// no other writer can interleave. Returns whatever `f` returns.
pub fn update<T>(
    app_handle: &tauri::AppHandle,
    f: impl FnOnce(&mut Config) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = config_path(app_handle);
    let mut cfg = load_from(&path).unwrap_or_default();
    let out = f(&mut cfg)?;
    write_to(&path, &cfg)?;
    Ok(out)
}

/// Write atomically: a crash mid-write leaves the old file intact. The file
/// being replaced becomes the backup if it's valid, or is set aside (never
/// deleted) if it's damaged.
fn write_to(path: &Path, config: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content).map_err(|e| e.to_string())?;

    match read_file(path) {
        Ok(Some(_)) => {
            if let Err(e) = fs::copy(path, backup_path(path)) {
                log::warn!("Could not back up config: {}", e);
            }
        }
        Ok(None) => {}
        Err(_) => {
            let aside = path.with_extension(format!("corrupt-{}.json", chrono::Utc::now().format("%Y%m%d%H%M%S")));
            log::error!("Replacing an unreadable config; the old file is kept at {}", aside.display());
            let _ = fs::copy(path, &aside);
        }
    }

    // std::fs::rename replaces the target on Windows too (MoveFileEx).
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn delete_config(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let path = config_path(app_handle);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Delete the config once no printers remain — but keep it while scales are
/// still configured, so removing the last printer doesn't lose them.
pub fn delete_config_if_unused(app_handle: &tauri::AppHandle) -> Result<(), String> {
    match load_config(app_handle) {
        Some(cfg) if !cfg.scales.is_empty() => Ok(()),
        _ => delete_config(app_handle),
    }
}

/// Insert the agent, or replace the existing entry with the same agent_id
/// (never creates duplicates).
pub fn add_agent(app_handle: &tauri::AppHandle, agent: AgentConfig) -> Result<(), String> {
    update(app_handle, |config| {
        match config.agents.iter_mut().find(|a| a.agent_id == agent.agent_id) {
            Some(existing) => *existing = agent,
            None => config.agents.push(agent),
        }
        Ok(())
    })
}

pub fn remove_agent(app_handle: &tauri::AppHandle, agent_id: &str) -> Result<(), String> {
    update(app_handle, |config| {
        config.agents.retain(|a| a.agent_id != agent_id);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("print-agent-cfg-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn with_scale(id: &str) -> Config {
        let mut c = Config::default();
        c.scales.push(serde_json::from_value(serde_json::json!({ "id": id })).unwrap());
        c
    }

    #[test]
    fn damaged_config_falls_back_to_backup() {
        let path = dir("fallback").join("config.json");
        write_to(&path, &with_scale("a")).unwrap();
        write_to(&path, &with_scale("b")).unwrap();
        // Backup holds the previous good version.
        assert_eq!(load_from(&backup_path(&path)).unwrap().scales[0].id, "a");

        fs::write(&path, "{ truncated").unwrap();
        assert_eq!(load_from(&path).unwrap().scales[0].id, "a");
    }

    #[test]
    fn overwriting_damaged_config_keeps_a_copy() {
        let d = dir("aside");
        let path = d.join("config.json");
        fs::write(&path, "not json").unwrap();
        write_to(&path, &with_scale("c")).unwrap();
        assert_eq!(load_from(&path).unwrap().scales[0].id, "c");
        let kept = fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("corrupt-"));
        assert!(kept);
    }
}
