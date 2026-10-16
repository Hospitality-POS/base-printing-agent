use printer_core::PrintTarget;
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    // `print-agent-helper --check` prints a compatibility report and exits.
    if args.iter().any(|a| a == "--check" || a == "-c") {
        println!("PrintAgent helper compatibility check\n");
        println!("{}", printer_core::compatibility_report());
        return;
    }

    let port = args
        .windows(2)
        .find(|w| w[0] == "--port")
        .and_then(|w| w[1].parse::<u16>().ok())
        .unwrap_or(14200);

    if args.iter().any(|a| a == "--install") {
        std::process::exit(install_wizard());
    }
    if args.iter().any(|a| a == "--uninstall") {
        std::process::exit(uninstall());
    }
    if args.iter().any(|a| a == "--open" || a == "-o") {
        std::process::exit(open_app(port));
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("PrintAgent helper — localhost print bridge for the web UI");
        println!("Usage:");
        println!("  print-agent-helper              run the helper server (port 14200)");
        println!("  print-agent-helper --open       start the helper and open the setup UI");
        println!("  print-agent-helper --install    setup wizard (copy files, autostart)");
        println!("  print-agent-helper --uninstall  remove autostart + installed files");
        println!("  print-agent-helper --check      print OS/printer compatibility report");
        println!("  print-agent-helper --port <n>   listen on a custom port");
        return;
    }
    let addr = format!("127.0.0.1:{}", port);

    let server = match tiny_http::Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to bind {}: {}", addr, e);
            std::process::exit(1);
        }
    };
    println!("PrintAgent helper listening on http://{}", addr);

    for mut request in server.incoming_requests() {
        let url = request.url().to_string();
        let method = request.method().as_str().to_string();

        let response = match (method.as_str(), url.as_str()) {
            ("OPTIONS", _) => tiny_http::Response::from_string("").with_status_code(204),
            ("GET", "/health") => json_response(&json!({
                "ok": true,
                "name": "print-agent-helper",
                "version": env!("CARGO_PKG_VERSION"),
            })),
            ("GET", "/os") => json_response(&serde_json::to_value(printer_core::get_os_info()).unwrap()),
            ("POST", "/cmd") => {
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_err() {
                    error_response("failed to read request body")
                } else {
                    match serde_json::from_str::<Value>(&body) {
                        Ok(payload) => {
                            let cmd = payload["cmd"].as_str().unwrap_or("").to_string();
                            let cmd_args = payload["args"].clone();
                            dispatch(&cmd, &cmd_args)
                        }
                        Err(e) => error_response(&format!("invalid JSON: {}", e)),
                    }
                }
            }
            (m, path) if path.starts_with("/agent-api") => proxy_api(&m, path, &mut request),
            ("GET", path) => serve_static(path),
            _ => tiny_http::Response::from_string("not found").with_status_code(404),
        };

        let response = with_cors(response);
        let _ = request.respond(response);
    }
}

const REMOTE_API: &str = "https://api.hospitality.reliatech.co.ke";

fn helper_config_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("print-agent-helper.json")))
        .unwrap_or_else(|| std::path::PathBuf::from("print-agent-helper.json"))
}

fn load_helper_config() -> Value {
    std::fs::read_to_string(helper_config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

fn save_helper_config(mut cfg: Value) -> Result<(), String> {
    if !cfg.is_object() {
        cfg = json!({});
    }
    let body = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(helper_config_path(), body).map_err(|e| e.to_string())
}

fn configured_api_base() -> String {
    load_helper_config()["apiUrl"]
        .as_str()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(|u| u.trim_end_matches('/').to_string())
        .or_else(|| std::env::var("PRINT_AGENT_API_URL").ok())
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| REMOTE_API.to_string())
}

fn set_helper_api_url(url: Option<&str>) -> Result<(), String> {
    let cleaned = url
        .map(str::trim)
        .map(|u| u.trim_end_matches('/'))
        .filter(|u| !u.is_empty())
        .map(str::to_string);
    let mut cfg = load_helper_config();
    if !cfg.is_object() {
        cfg = json!({});
    }
    match cleaned {
        Some(u) => cfg["apiUrl"] = json!(u),
        None => {
            if let Some(obj) = cfg.as_object_mut() {
                obj.remove("apiUrl");
            }
        }
    }
    save_helper_config(cfg)
}

/// Proxy /agent-api/* to the remote API. Keeps everything same-origin (no CORS)
/// and gives legacy machines rustls TLS instead of relying on old system TLS.
fn proxy_api(
    method: &str,
    path: &str,
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let api_base = configured_api_base();
    let url = format!("{}{}", api_base, &path["/agent-api".len()..]);

    let mut req = ureq::request(method, &url).timeout(Duration::from_secs(20));
    if let Some(cc) = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("companycode"))
    {
        req = req.set("companycode", cc.value.as_str());
    }

    let has_body = method == "POST" || method == "PUT" || method == "PATCH";
    let result = if has_body {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        req.set("Content-Type", "application/json").send_string(&body)
    } else {
        req.call()
    };

    match result {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.into_string().unwrap_or_default();
            tiny_http::Response::from_string(text).with_status_code(status)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            tiny_http::Response::from_string(text).with_status_code(code)
        }
        Err(e) => error_response(&format!("API proxy failed: {}", e)),
    }
}

/// Serve the built web UI (`npm run build` output) from a `web/` folder placed
/// next to the helper exe, falling back to `dist/` for convenience.
fn web_root() -> Option<std::path::PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    for dir in [exe_dir.join("web"), exe_dir.join("dist"), std::path::PathBuf::from("web"), std::path::PathBuf::from("dist")] {
        if dir.join("index.html").exists() {
            return Some(dir);
        }
    }
    None
}

fn serve_static(path: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let root = match web_root() {
        Some(r) => r,
        None => {
            return tiny_http::Response::from_string(
                "web UI not found — place the built 'dist' folder next to print-agent-helper",
            )
            .with_status_code(404)
        }
    };

    // Strip query string, prevent path traversal, default to index.html
    let clean = path.split('?').next().unwrap_or("/");
    let rel = clean.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    let file_path = if !rel.split('/').any(|seg| seg == "..") && root.join(rel).exists() {
        root.join(rel)
    } else {
        // SPA fallback: unknown paths serve index.html
        root.join("index.html")
    };

    match std::fs::read(&file_path) {
        Ok(bytes) => {
            let mime = match file_path.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html",
                Some("js") | Some("mjs") => "application/javascript",
                Some("css") => "text/css",
                Some("json") => "application/json",
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("svg") => "image/svg+xml",
                Some("ico") => "image/x-icon",
                Some("woff") | Some("woff2") => "font/woff2",
                _ => "application/octet-stream",
            };
            tiny_http::Response::from_data(bytes).with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap(),
            )
        }
        Err(_) => tiny_http::Response::from_string("not found").with_status_code(404),
    }
}

fn with_cors(response: tiny_http::Response<std::io::Cursor<Vec<u8>>>) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let mut r = response;
    for (name, value) in [
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Methods", "GET, POST, PUT, PATCH, DELETE, OPTIONS"),
        ("Access-Control-Allow-Headers", "Content-Type, companycode"),
    ] {
        if let Ok(h) = tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            r.add_header(h);
        }
    }
    r
}

fn json_response(value: &Value) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    tiny_http::Response::from_string(body).with_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
    )
}

fn open_app(port: u16) -> i32 {
    let addr: SocketAddr = match format!("127.0.0.1:{}", port).parse() {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("Invalid helper port {}: {}", port, e);
            return 1;
        }
    };

    let is_running = || TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok();

    if !is_running() {
        let exe = current_exe();
        if let Err(e) = std::process::Command::new(&exe).spawn() {
            eprintln!("Failed to start PrintAgent helper: {}", e);
            return 1;
        }

        // Older machines can take a few seconds to bind the local server.
        let mut started = false;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(250));
            if is_running() {
                started = true;
                break;
            }
        }
        if !started {
            eprintln!("PrintAgent helper did not start on port {}", port);
            return 1;
        }
    }

    let url = format!("http://127.0.0.1:{}", port);
    let opened = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn();
    if opened.is_err() {
        if let Err(e) = std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .spawn()
        {
            eprintln!("Failed to open {}: {}", url, e);
            return 1;
        }
    }
    0
}

fn ok_response(result: Value) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    json_response(&json!({ "ok": true, "result": result }))
}

fn error_response(msg: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    json_response(&json!({ "ok": false, "error": msg }))
}

fn dispatch(cmd: &str, args: &Value) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    match cmd {
        "get_available_ports" => {
            ok_response(json!(printer_core::get_available_ports()))
        }
        "detect_thermal_printers" => {
            ok_response(serde_json::to_value(printer_core::detect_thermal_printers()).unwrap())
        }
        "list_windows_printers" => {
            ok_response(json!(printer_core::list_windows_printers()))
        }
        "discover_network_printers" => {
            ok_response(json!(printer_core::discover_network_printers()))
        }
        "get_os_version" | "get_windows_version" => {
            ok_response(serde_json::to_value(printer_core::get_os_info()).unwrap())
        }
        "get_api_url" => ok_response(json!(configured_api_base())),
        "set_api_url" => match set_helper_api_url(args["url"].as_str()) {
            Ok(()) => ok_response(Value::Null),
            Err(e) => error_response(&e),
        },
        "compatibility_report" => {
            ok_response(json!(printer_core::compatibility_report()))
        }
        "get_autostart" => ok_response(json!(autostart_enabled())),
        "set_autostart" => {
            let enabled = args["enabled"].as_bool().unwrap_or(false);
            match set_autostart_at(&current_exe(), enabled) {
                Ok(()) => ok_response(Value::Null),
                Err(e) => error_response(&e),
            }
        }
        "test_print" => match target_from_args(args) {
            Ok(target) => match printer_core::test_print(&target) {
                Ok(()) => ok_response(Value::Null),
                Err(e) => error_response(&e),
            },
            Err(e) => error_response(&e),
        },
        "print_job" => {
            let job: Result<printer_core::PrintJob, _> =
                serde_json::from_value(args["job"].clone());
            match (job, target_from_args(args)) {
                (Ok(job), Ok(target)) => match printer_core::print_job(&job, &target) {
                    Ok(()) => ok_response(Value::Null),
                    Err(e) => error_response(&e),
                },
                (Err(e), _) => error_response(&format!("invalid job payload: {}", e)),
                (_, Err(e)) => error_response(&e),
            }
        }
        "test_printer_connection" => {
            let host = args["printerHost"].as_str().unwrap_or("");
            let port = args["printerNetworkPort"].as_u64().unwrap_or(9100) as u16;
            match test_tcp(host, port) {
                Ok(msg) => ok_response(json!(msg)),
                Err(e) => error_response(&e),
            }
        }
        other => error_response(&format!("unknown command: {}", other)),
    }
}

// ---------------------------------------------------------------------------
// Autostart — HKCU Run key works on Windows 7 through 11 with no admin rights.
// ---------------------------------------------------------------------------

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "PrintAgentHelper";

fn current_exe() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "print-agent-helper.exe".to_string())
}

#[cfg(target_os = "windows")]
fn autostart_enabled() -> bool {
    std::process::Command::new("reg")
        .args(["query", RUN_KEY, "/v", RUN_VALUE])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn autostart_enabled() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn set_autostart_at(exe_path: &str, enable: bool) -> Result<(), String> {
    use std::process::Command;
    if enable {
        let status = Command::new("reg")
            .args([
                "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ",
                "/d", &format!("\"{}\"", exe_path), "/f",
            ])
            .output()
            .map_err(|e| format!("Failed to run reg: {}", e))?;
        if !status.status.success() {
            return Err(String::from_utf8_lossy(&status.stderr).trim().to_string());
        }
    } else {
        // Ignore failure — value may simply not exist
        let _ = Command::new("reg")
            .args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])
            .output();
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_autostart_at(_exe_path: &str, _enable: bool) -> Result<(), String> {
    Err("Autostart is only supported on Windows".to_string())
}

// ---------------------------------------------------------------------------
// Install / uninstall wizard
// ---------------------------------------------------------------------------

fn prompt(question: &str, default: &str) -> String {
    use std::io::Write;
    if default.is_empty() {
        print!("{}: ", question);
    } else {
        print!("{} [{}]: ", question, default);
    }
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    let line = line.trim();
    if line.is_empty() {
        default.to_string()
    } else {
        line.to_string()
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

fn default_install_dir() -> std::path::PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(|p| std::path::PathBuf::from(p).join("PrintAgent"))
        .unwrap_or_else(|_| std::path::PathBuf::from(r"C:\PrintAgent"))
}

fn install_wizard() -> i32 {
    println!("=== PrintAgent Setup Wizard ===\n");

    let exe = current_exe();
    let exe_dir = std::path::Path::new(&exe)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();

    // Locate the web UI folder shipped next to the exe (web/ or dist/)
    let web_src = [exe_dir.join("web"), exe_dir.join("dist")]
        .into_iter()
        .find(|p| p.join("index.html").exists());

    let install_dir = prompt(
        "Install folder",
        &default_install_dir().display().to_string(),
    );
    let install_dir = std::path::PathBuf::from(install_dir);

    println!("\nInstalling to {}...", install_dir.display());
    if let Err(e) = std::fs::create_dir_all(&install_dir) {
        eprintln!("Failed to create install folder: {}", e);
        return 1;
    }

    // Copy the helper exe
    let dest_exe = install_dir.join("print-agent-helper.exe");
    if let Err(e) = std::fs::copy(&exe, &dest_exe) {
        eprintln!("Failed to copy helper: {}", e);
        return 1;
    }
    println!("  copied print-agent-helper.exe");

    // Copy the web UI
    if let Some(src) = web_src {
        let dest_web = install_dir.join("web");
        match copy_dir(&src, &dest_web) {
            Ok(()) => println!("  copied web UI -> {}", dest_web.display()),
            Err(e) => eprintln!("  warning: failed to copy web UI: {}", e),
        }
    } else {
        println!("  note: no web/dist folder found next to the exe — the UI won't be served locally");
    }

    // Autostart choice
    let autostart = prompt("\nStart PrintAgent automatically at login? (y/n)", "y");
    if autostart.eq_ignore_ascii_case("y") {
        match set_autostart_at(&dest_exe.display().to_string(), true) {
            Ok(()) => println!("  autostart enabled (current user)"),
            Err(e) => eprintln!("  warning: failed to enable autostart: {}", e),
        }
    }

    // Start now?
    let start_now = prompt("\nStart PrintAgent now? (y/n)", "y");
    if start_now.eq_ignore_ascii_case("y") {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", &dest_exe.display().to_string()])
            .spawn();
        let open = prompt("Open the setup page in your browser? (y/n)", "y");
        if open.eq_ignore_ascii_case("y") {
            let _ = std::process::Command::new("cmd")
                .args(["/c", "start", "", "http://localhost:14200"])
                .spawn();
        }
    }

    println!("\nDone. PrintAgent will listen on http://localhost:14200");
    let _ = prompt("Press Enter to exit", "");
    0
}

fn uninstall() -> i32 {
    println!("=== PrintAgent Uninstall ===\n");
    match set_autostart_at(&current_exe(), false) {
        Ok(()) => println!("Autostart entry removed."),
        Err(e) => eprintln!("Failed to remove autostart: {}", e),
    }
    let remove_files = prompt(
        "Also delete installed files? (y/n)",
        "n",
    );
    if remove_files.eq_ignore_ascii_case("y") {
        let dir = prompt("Install folder to delete", &default_install_dir().display().to_string());
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => println!("Deleted {}", dir),
            Err(e) => eprintln!("Failed to delete {}: {}", dir, e),
        }
    }
    println!("Uninstall complete.");
    0
}

fn target_from_args(args: &Value) -> Result<PrintTarget, String> {
    let printer_type = args["printerType"].as_str().unwrap_or("serial");
    let printer_port = args["printerPort"].as_str().unwrap_or("");
    let printer_host = args["printerHost"].as_str();
    let printer_network_port = args["printerNetworkPort"].as_i64();

    if printer_type != "network" && printer_port.is_empty() {
        return Err("printerPort is required".to_string());
    }
    Ok(PrintTarget::from_fields(
        printer_type,
        printer_port,
        printer_host,
        printer_network_port,
    ))
}

fn test_tcp(host: &str, port: u16) -> Result<String, String> {
    let addr_str = format!("{}:{}", host, port);
    let socket_addr: SocketAddr = match addr_str.parse() {
        Ok(a) => a,
        Err(_) => match (host, port).to_socket_addrs() {
            Ok(mut addrs) => match addrs.next() {
                Some(a) => a,
                None => return Err(format!("No addresses found for {}", host)),
            },
            Err(e) => return Err(format!("Failed to resolve address: {}", e)),
        },
    };
    match TcpStream::connect_timeout(&socket_addr, Duration::from_secs(5)) {
        Ok(_) => Ok(format!(
            "Successfully connected to printer at {}",
            socket_addr
        )),
        Err(e) => Err(format!(
            "Failed to connect to printer at {}: {}",
            socket_addr, e
        )),
    }
}
