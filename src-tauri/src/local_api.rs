//! Localhost HTTP API for the POS web app: low-latency access to the scale,
//! cash drawer, printer status and direct printing without a cloud round
//! trip. Binds to 127.0.0.1 only; browser callers must come from an allowed
//! Origin (see `LocalApiConfig::allowed_origins`).
//!
//!   GET  /api/v1/health
//!   GET  /api/v1/devices
//!   GET  /api/v1/printers/{agentId}/status[?refresh=1]
//!   POST /api/v1/printers/{agentId}/print      body: PrintJob
//!   POST /api/v1/printers/{agentId}/label      body: LabelJob
//!   POST /api/v1/printers/{agentId}/drawer
//!   POST /api/v1/drawer                        body: {"agentId"?: "..."}
//!   POST /api/v1/label                         body: LabelJob (first label printer)
//!   GET  /api/v1/scales
//!   GET  /api/v1/scales/{scaleId|default}/weight
//!   POST /api/v1/scales/{scaleId|default}/tare
//!   POST /api/v1/scales/{scaleId|default}/zero

use crate::config::{self, LocalApiConfig};
use crate::devices;
use crate::AppState;
use serde_json::{json, Value};
use std::sync::Arc;

type Resp = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

/// Glob match where `*` matches any run of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] != '*' && p[pi].eq_ignore_ascii_case(&t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

pub fn origin_allowed(origin: Option<&str>, allowed: &[String]) -> bool {
    match origin {
        None => true,
        Some(o) => allowed.iter().any(|p| p == "*" || glob_match(p.trim().trim_end_matches('/'), o)),
    }
}

/// Start (or restart) the server with the current config.
pub fn start(state: Arc<AppState>) {
    stop(&state);
    let cfg: LocalApiConfig = config::load_config(&state.app_handle).unwrap_or_default().local_api;
    if !cfg.enabled {
        set_info(&state, json!({ "running": false, "port": cfg.port, "error": null }));
        log::info!("Local device API disabled");
        return;
    }
    let addr = format!("127.0.0.1:{}", cfg.port);
    let server = match tiny_http::Server::http(&addr) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            log::error!("Local device API could not bind {}: {}", addr, e);
            set_info(&state, json!({ "running": false, "port": cfg.port, "error": e.to_string() }));
            return;
        }
    };
    log::info!("Local device API listening on http://{}", addr);
    set_info(&state, json!({ "running": true, "port": cfg.port, "error": null }));
    if let Ok(mut slot) = state.local_api_server.lock() {
        *slot = Some(Arc::clone(&server));
    }

    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let state = Arc::clone(&state);
            // Device I/O can take a second or two; don't block other callers.
            std::thread::spawn(move || handle(request, &state));
        }
        log::info!("Local device API stopped");
    });
}

pub fn stop(state: &AppState) {
    if let Ok(mut slot) = state.local_api_server.lock() {
        if let Some(server) = slot.take() {
            server.unblock();
        }
    }
}

fn set_info(state: &AppState, info: Value) {
    if let Ok(mut i) = state.local_api_info.lock() {
        *i = info;
    }
}

fn header(name: &str, value: &str) -> Option<tiny_http::Header> {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

fn json_resp(status: u16, body: Value) -> Resp {
    let mut r = tiny_http::Response::from_string(body.to_string()).with_status_code(status);
    if let Some(h) = header("Content-Type", "application/json") {
        r.add_header(h);
    }
    r
}

fn ok(result: Value) -> Resp {
    json_resp(200, json!({ "ok": true, "result": result }))
}

fn err(status: u16, msg: impl Into<String>) -> Resp {
    json_resp(status, json!({ "ok": false, "error": msg.into() }))
}

fn from_result<T: serde::Serialize>(r: Result<T, String>) -> Resp {
    match r {
        Ok(v) => ok(serde_json::to_value(v).unwrap_or(Value::Null)),
        Err(e) => err(500, e),
    }
}

fn handle(mut request: tiny_http::Request, state: &AppState) {
    let origin = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Origin"))
        .map(|h| h.value.as_str().to_string());
    let allowed = config::load_config(&state.app_handle).unwrap_or_default().local_api.allowed_origins;
    let origin_ok = origin_allowed(origin.as_deref(), &allowed);

    let response = if !origin_ok {
        log::warn!("Local device API: blocked request from origin {:?}", origin);
        err(403, "Origin not allowed — add it to localApi.allowedOrigins in PrintAgent")
    } else if request.method() == &tiny_http::Method::Options {
        tiny_http::Response::from_string("").with_status_code(204)
    } else {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        route(request.method().as_str(), request.url(), &body, state)
    };

    let mut response = response;
    if let (true, Some(o)) = (origin_ok, origin.as_deref()) {
        for (k, v) in [
            ("Access-Control-Allow-Origin", o),
            ("Vary", "Origin"),
            ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
            ("Access-Control-Allow-Headers", "Content-Type"),
            // Chrome Private Network Access preflight for https → localhost.
            ("Access-Control-Allow-Private-Network", "true"),
        ] {
            if let Some(h) = header(k, v) {
                response.add_header(h);
            }
        }
    }
    let _ = request.respond(response);
}

fn parse_body<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, String> {
    serde_json::from_str(if body.trim().is_empty() { "{}" } else { body }).map_err(|e| format!("Invalid JSON body: {}", e))
}

fn route(method: &str, url: &str, body: &str, state: &AppState) -> Resp {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let segs: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    let segs: Vec<String> = segs.iter().map(|s| urlencoding::decode(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string())).collect();
    let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
    let refresh = query.split('&').any(|kv| kv == "refresh=1" || kv == "refresh=true");

    match (method, segs.as_slice()) {
        ("GET", ["api", "v1", "health"]) => ok(json!({
            "name": "print-agent",
            "version": env!("CARGO_PKG_VERSION"),
        })),
        ("GET", ["api", "v1", "devices"]) => {
            let cfg = config::load_config(&state.app_handle).unwrap_or_default();
            let statuses = devices::cached_statuses(state);
            ok(json!({
                "printers": cfg.agents.iter().map(|a| json!({
                    "agentId": a.agent_id,
                    "name": a.device_name,
                    "zones": a.all_zones(),
                    "language": a.language().as_str(),
                    "cashDrawer": a.cash_drawer,
                    "status": statuses.get(&a.agent_id).map(|s| &s.status),
                })).collect::<Vec<_>>(),
                "scales": cfg.scales.iter().map(|s| json!({
                    "id": s.id, "name": s.name, "protocol": s.protocol, "unit": s.unit,
                })).collect::<Vec<_>>(),
            }))
        }
        ("GET", ["api", "v1", "printers", id, "status"]) => {
            if !refresh {
                if let Some(entry) = devices::cached_statuses(state).remove(*id) {
                    return ok(serde_json::to_value(entry).unwrap_or(Value::Null));
                }
            }
            from_result(devices::status_blocking(state, id))
        }
        ("POST", ["api", "v1", "printers", id, "drawer"]) => {
            from_result(devices::open_drawer_blocking(state, Some(id)).map(|a| json!({ "agentId": a })))
        }
        ("POST", ["api", "v1", "drawer"]) => {
            let b: Value = match parse_body(body) {
                Ok(v) => v,
                Err(e) => return err(400, e),
            };
            from_result(devices::open_drawer_blocking(state, b["agentId"].as_str()).map(|a| json!({ "agentId": a })))
        }
        ("POST", ["api", "v1", "printers", id, "print"]) => match parse_body::<crate::printer::PrintJob>(body) {
            Ok(job) => from_result(devices::print_job_blocking(state, id, &job)),
            Err(e) => err(400, e),
        },
        ("POST", ["api", "v1", "printers", id, "label"]) => match parse_body::<crate::printer::LabelJob>(body) {
            Ok(label) => from_result(devices::print_label_blocking(state, Some(id), &label).map(|a| json!({ "agentId": a }))),
            Err(e) => err(400, e),
        },
        ("POST", ["api", "v1", "label"]) => match parse_body::<crate::printer::LabelJob>(body) {
            Ok(label) => from_result(devices::print_label_blocking(state, None, &label).map(|a| json!({ "agentId": a }))),
            Err(e) => err(400, e),
        },
        ("GET", ["api", "v1", "scales"]) => {
            let cfg = config::load_config(&state.app_handle).unwrap_or_default();
            ok(serde_json::to_value(&cfg.scales).unwrap_or(Value::Null))
        }
        ("GET", ["api", "v1", "scales", id, "weight"]) => from_result(
            devices::read_scale_blocking(state, Some(id)).map(|(scale_id, r)| {
                let mut v = serde_json::to_value(r).unwrap_or(Value::Null);
                v["scaleId"] = json!(scale_id);
                v
            }),
        ),
        ("POST", ["api", "v1", "scales", id, cmd @ ("tare" | "zero")]) => {
            from_result(devices::scale_command_blocking(state, Some(id), cmd))
        }
        _ => err(404, format!("No route for {} {}", method, path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_rules() {
        let allowed = config::default_allowed_origins();
        assert!(origin_allowed(None, &allowed));
        assert!(origin_allowed(Some("https://pos.reliatech.co.ke"), &allowed));
        assert!(origin_allowed(Some("https://a.b.reliatech.co.ke"), &allowed));
        assert!(origin_allowed(Some("http://localhost:5173"), &allowed));
        assert!(origin_allowed(Some("http://localhost"), &allowed));
        assert!(!origin_allowed(Some("https://evilreliatech.co.ke"), &allowed));
        assert!(!origin_allowed(Some("https://reliatech.co.ke.evil.com"), &allowed));
        assert!(!origin_allowed(Some("http://localhost.evil.com"), &allowed));
        assert!(origin_allowed(Some("https://anything"), &["*".to_string()]));
    }
}
