pub mod commands;
pub mod config;
pub mod job_handler;
pub mod printer;
pub mod ws_client;

use std::sync::{Arc, Mutex};
use tauri::Manager;
use tokio::sync::Mutex as AsyncMutex;

pub struct AppState {
    pub agent_connections: AsyncMutex<std::collections::HashMap<String, AgentConnection>>,
    pub last_job_id: Mutex<Option<String>>,
    pub last_job_time: Mutex<Option<String>>,
    pub app_handle: tauri::AppHandle,
}

pub struct AgentConnection {
    pub connected: bool,
    pub sender: Option<tokio::sync::mpsc::UnboundedSender<String>>,
}

impl AppState {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        AppState {
            agent_connections: AsyncMutex::new(std::collections::HashMap::new()),
            last_job_id: Mutex::new(None),
            last_job_time: Mutex::new(None),
            app_handle,
        }
    }
}

pub async fn start_ws_loop(state: Arc<AppState>, cfg: &config::Config) {
    log::info!("start_ws_loop called with {} agents", cfg.agents.len());

    // Deduplicate agents by agent_id to avoid duplicate polling loops
    let mut seen_agents = std::collections::HashSet::new();
    let mut unique_agents = Vec::new();
    for agent in &cfg.agents {
        if seen_agents.insert(&agent.agent_id) {
            unique_agents.push(agent);
        } else {
            log::warn!("Skipping duplicate agent: {}", agent.agent_id);
        }
    }
    log::info!("After deduplication: {} unique agents", unique_agents.len());

    for agent in unique_agents {
        let printer_target = match agent.printer_type.as_str() {
            "network" => {
                if let (Some(host), Some(port)) = (&agent.printer_host, agent.printer_network_port) {
                    printer::PrintTarget::Network {
                        host: host.clone(),
                        port: port as u16,
                    }
                } else {
                    log::warn!("Network printer for {} missing host or port, skipping", agent.agent_id);
                    continue;
                }
            }
            _ => printer::PrintTarget::Serial(agent.printer_port.clone()),
        };

        // Only start WebSocket loop if agent has valid credentials
        if !agent.ws_url.is_empty() && !agent.device_token.is_empty() {
            log::info!("Starting WebSocket loop for agent: {}", agent.agent_id);
            tokio::spawn(ws_client::run_ws_loop(
                Arc::clone(&state),
                agent.ws_url.clone(),
                agent.device_token.clone(),
                agent.agent_id.clone(),
                agent.zone.clone(),
                agent.branch_id.clone(),
                agent.company_code.clone(),
                printer_target.clone(),
            ));
        } else {
            log::info!("Agent {} missing WebSocket credentials, skipping WebSocket connection", agent.agent_id);
        }

        // Start polling loop for pull-based job fetching
        if !agent.branch_id.is_empty() && !agent.zone.is_empty() {
            // Check if printer configuration is valid
            let has_valid_printer = match agent.printer_type.as_str() {
                "network" => {
                    agent.printer_host.as_ref().map_or(false, |h| !h.is_empty())
                        && agent.printer_network_port.is_some()
                }
                _ => !agent.printer_port.is_empty(),
            };

            if has_valid_printer {
                log::info!("Starting polling loop for agent: {} (zone: {})", agent.agent_id, agent.zone);
                tokio::spawn(ws_client::start_polling_loop(
                    Arc::clone(&state),
                    agent.branch_id.clone(),
                    agent.zone.clone(),
                    agent.company_code.clone(),
                    agent.agent_id.clone(),
                ));
            } else {
                log::warn!("Agent {} has invalid printer configuration, skipping polling", agent.agent_id);
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let state = Arc::new(AppState::new(app_handle.clone()));
            let state_clone = Arc::clone(&state);

            setup_tray(&app_handle);

            tauri::async_runtime::spawn(async move {
                setup_app(app_handle, state_clone).await;
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_available_ports,
            commands::discover_network_printers,
            commands::fetch_shops,
            commands::list_agents,
            commands::fetch_connected_agents,
            commands::fetch_agents_from_api,
            commands::auto_add_connected_agent,
            commands::connect_to_existing_agent,
            commands::fetch_pending_jobs,
            commands::update_agent_printer_port,
            commands::get_config,
            commands::get_agent_status,
            commands::pair_device,
            commands::test_print,
            commands::unpair_device,
            commands::test_printer_connection,
            commands::remove_duplicate_agents,
        ])
        .run(tauri::generate_context!())
        .expect("Error running PrintAgent");
}

async fn setup_app(app_handle: tauri::AppHandle, state: Arc<AppState>) {
    let cfg = config::load_config(&app_handle);

    let window = app_handle.get_webview_window("main").unwrap();

    if cfg.is_none() {
        window.show().unwrap();
        window.set_focus().unwrap();
    }

    if let Some(ref c) = cfg {
        start_ws_loop(Arc::clone(&state), c).await;
    }
}

fn setup_tray(app_handle: &tauri::AppHandle) {
    use tauri::image::Image;
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let quit = MenuItem::with_id(app_handle, "quit", "Quit PrintAgent", true, None::<&str>)
        .expect("Failed to create quit menu item");
    let show =
        MenuItem::with_id(app_handle, "show", "Open Status", true, None::<&str>)
            .expect("Failed to create show menu item");
    let test_print_item =
        MenuItem::with_id(app_handle, "test_print", "Test Print", true, None::<&str>)
            .expect("Failed to create test_print menu item");

    let menu = Menu::with_items(app_handle, &[&show, &test_print_item, &quit])
        .expect("Failed to create tray menu");

    let app_handle_clone = app_handle.clone();

    let icon_bytes = include_bytes!("../icons/32x32.png");
    let icon = Image::from_bytes(icon_bytes).expect("Failed to load tray icon");

    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("PrintAgent — Silent Print Routing")
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = app_handle_clone.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "test_print" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let cfg = config::load_config(&app);
                    if let Some(ref c) = cfg {
                        if !c.agents.is_empty() {
                            let target = printer::PrintTarget::from_agent_config(&c.agents[0]);
                            if let Err(e) = printer::test_print(&target) {
                                log::error!("Tray test print failed: {}", e);
                            }
                        }
                    }
                });
            }
            "quit" => {
                std::process::exit(0);
            }
            _ => {}
        })
        .build(app_handle)
        .expect("Failed to build tray icon");
}
