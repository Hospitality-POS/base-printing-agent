pub mod commands;
pub mod config;
pub mod job_handler;
pub mod job_queue;
pub mod printer;
pub mod ws_client;

use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tokio::sync::Mutex as AsyncMutex;

pub struct AppState {
    pub agent_connections: AsyncMutex<std::collections::HashMap<String, AgentConnection>>,
    pub last_job_id: Mutex<Option<String>>,
    pub last_job_time: Mutex<Option<String>>,
    pub db_pool: AsyncMutex<Option<SqlitePool>>,
}

pub struct AgentConnection {
    pub connected: bool,
    pub sender: Option<tokio::sync::mpsc::UnboundedSender<String>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            agent_connections: AsyncMutex::new(std::collections::HashMap::new()),
            last_job_id: Mutex::new(None),
            last_job_time: Mutex::new(None),
            db_pool: AsyncMutex::new(None),
        }
    }
}

pub async fn start_ws_loop(state: Arc<AppState>, cfg: &config::Config) {
    log::info!("start_ws_loop called with {} agents", cfg.agents.len());

    let pool_lock = state.db_pool.lock().await;
    let pool = match pool_lock.as_ref() {
        Some(p) => p.clone(),
        None => {
            log::error!("DB pool not ready, cannot start WS loop.");
            return;
        }
    };
    drop(pool_lock);

    for agent in &cfg.agents {
        log::info!("Starting WebSocket loop for agent: {} (zone: {})", agent.agent_id, agent.zone);
        let printer_target = printer::PrintTarget::from_agent_config(agent);

        tokio::spawn(ws_client::run_ws_loop(
            Arc::clone(&state),
            pool.clone(),
            agent.ws_url.clone(),
            agent.device_token.clone(),
            agent.agent_id.clone(),
            agent.zone.clone(),
            agent.branch_id.clone(),
            agent.company_code.clone(),
            printer_target,
        ));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = Arc::new(AppState::new());
    let state_clone = Arc::clone(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let state = Arc::clone(&state_clone);

            setup_tray(&app_handle);

            tauri::async_runtime::spawn(async move {
                setup_app(app_handle, state).await;
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
            commands::get_config,
            commands::get_agent_status,
            commands::pair_device,
            commands::test_print,
            commands::unpair_device,
        ])
        .run(tauri::generate_context!())
        .expect("Error running PrintAgent");
}

async fn setup_app(app_handle: tauri::AppHandle, state: Arc<AppState>) {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .expect("Cannot get app data dir");

    std::fs::create_dir_all(&data_dir).expect("Cannot create data dir");

    let db_path = data_dir.join("jobs.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    match job_queue::init_db(&db_url).await {
        Ok(pool) => {
            log::info!("SQLite DB initialized at {:?}", db_path);
            let mut pool_lock = state.db_pool.lock().await;
            *pool_lock = Some(pool);
        }
        Err(e) => {
            log::error!("Failed to init DB: {}", e);
            return;
        }
    }

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
