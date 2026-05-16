pub mod commands;
pub mod config;
pub mod job_handler;
pub mod job_queue;
pub mod printer;
pub mod ws_client;

use crate::config::Config;
use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tokio::sync::Mutex as AsyncMutex;

pub struct AppState {
    pub ws_connected: Mutex<bool>,
    pub ws_sender: AsyncMutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>,
    pub last_job_id: Mutex<Option<String>>,
    pub last_job_time: Mutex<Option<String>>,
    pub db_pool: AsyncMutex<Option<SqlitePool>>,
    pub current_config: AsyncMutex<Option<Config>>,
    pub ws_loop_started: Mutex<bool>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            ws_connected: Mutex::new(false),
            ws_sender: AsyncMutex::new(None),
            last_job_id: Mutex::new(None),
            last_job_time: Mutex::new(None),
            db_pool: AsyncMutex::new(None),
            current_config: AsyncMutex::new(None),
            ws_loop_started: Mutex::new(false),
        }
    }
}

pub async fn start_ws_loop(state: Arc<AppState>, cfg: &Config) {
    {
        let mut flag = state.ws_loop_started.lock().unwrap();
        if *flag {
            return;
        }
        *flag = true;
    }

    let pool_lock = state.db_pool.lock().await;
    let pool = match pool_lock.as_ref() {
        Some(p) => p.clone(),
        None => {
            log::error!("DB pool not ready, cannot start WS loop.");
            return;
        }
    };
    drop(pool_lock);

    tokio::spawn(ws_client::run_ws_loop(
        Arc::clone(&state),
        pool,
        cfg.ws_url.clone(),
        cfg.device_token.clone(),
        cfg.agent_id.clone(),
        cfg.zone.clone(),
        cfg.branch_id.clone(),
        cfg.printer_port.clone(),
    ));
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

    if let Some(ref c) = cfg {
        let mut cfg_lock = state.current_config.lock().await;
        *cfg_lock = Some(c.clone());
    }

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
                    let state = app.state::<Arc<AppState>>();
                    let cfg_lock = state.current_config.lock().await;
                    if let Some(cfg) = cfg_lock.as_ref() {
                        let port = cfg.printer_port.clone();
                        drop(cfg_lock);
                        if let Err(e) = crate::printer::test_print(&port) {
                            log::error!("Tray test print failed: {}", e);
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
