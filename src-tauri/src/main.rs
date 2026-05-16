#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let _ = dotenvy::dotenv(); // loads .env next to the exe, silent if missing
    env_logger::init();
    print_agent_lib::run();
}
