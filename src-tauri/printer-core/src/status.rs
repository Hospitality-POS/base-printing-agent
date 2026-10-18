//! Printer status: ESC/POS real-time status (DLE EOT) for serial and network
//! printers, and the Windows spooler's view for driver-installed printers.

use crate::transport::{self, Duplex, SerialSettings};
use crate::PrintTarget;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DLE: u8 = 0x10;
const EOT: u8 = 0x04;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterStatus {
    /// "ok" | "warning" | "error" | "offline" | "unknown"
    pub state: String,
    /// The port/socket could be opened (or the spooler knows the printer).
    pub reachable: bool,
    /// Where the details came from: "escpos" | "spooler" | "tcp" | "none".
    pub source: String,
    pub online: Option<bool>,
    pub cover_open: Option<bool>,
    pub paper_out: Option<bool>,
    pub paper_near_end: Option<bool>,
    pub error: Option<bool>,
    /// Raw level of the drawer-kick connector pin 3. Whether HIGH means
    /// "open" or "closed" depends on the drawer model.
    pub drawer_pin_high: Option<bool>,
    /// Jobs waiting in the Windows print queue (spooler printers only).
    pub queued_jobs: Option<u32>,
    pub messages: Vec<String>,
}

impl PrinterStatus {
    fn finish(mut self) -> Self {
        let bad = |v: Option<bool>| v == Some(true);
        if bad(self.paper_out) {
            self.messages.push("Out of paper".into());
        }
        if bad(self.cover_open) {
            self.messages.push("Cover is open".into());
        }
        if bad(self.error) {
            self.messages.push("Printer reported an error".into());
        }
        if self.online == Some(false) && self.messages.is_empty() {
            self.messages.push("Printer is offline".into());
        }
        if bad(self.paper_near_end) {
            self.messages.push("Paper is running low".into());
        }

        self.state = if !self.reachable {
            "offline"
        } else if bad(self.paper_out)
            || bad(self.cover_open)
            || bad(self.error)
            || self.online == Some(false)
        {
            "error"
        } else if bad(self.paper_near_end) {
            "warning"
        } else if self.source == "none" {
            "unknown"
        } else {
            "ok"
        }
        .to_string();
        self
    }
}

/// A DLE EOT reply byte always has bits 1 and 4 set and bits 0 and 7 clear.
fn valid(b: Option<u8>) -> Option<u8> {
    b.filter(|b| b & 0x93 == 0x12)
}

fn bit(b: Option<u8>, n: u8) -> Option<bool> {
    b.map(|b| b & (1 << n) != 0)
}

/// Decode the four DLE EOT n replies (n = 1 printer, 2 offline cause,
/// 3 error cause, 4 paper roll sensor). Missing/invalid bytes stay `None`.
pub fn parse_escpos_status(
    printer: Option<u8>,
    offline: Option<u8>,
    error: Option<u8>,
    paper: Option<u8>,
) -> PrinterStatus {
    let (printer, offline, error, paper) = (valid(printer), valid(offline), valid(error), valid(paper));
    let answered = printer.is_some() || offline.is_some() || error.is_some() || paper.is_some();

    let paper_out = match (bit(offline, 5), paper.map(|p| p & 0x60 == 0x60)) {
        (None, None) => None,
        (a, b) => Some(a == Some(true) || b == Some(true)),
    };
    let error_flag = match (bit(offline, 6), error.map(|e| e & 0x6C != 0)) {
        (None, None) => None,
        (a, b) => Some(a == Some(true) || b == Some(true)),
    };

    PrinterStatus {
        reachable: true,
        source: if answered { "escpos" } else { "none" }.into(),
        online: bit(printer, 3).map(|offline| !offline),
        cover_open: bit(offline, 2),
        paper_out,
        paper_near_end: paper.map(|p| p & 0x0C == 0x0C),
        error: error_flag,
        drawer_pin_high: bit(printer, 2),
        ..Default::default()
    }
    .finish()
}

fn query_escpos(conn: &mut dyn Duplex) -> [Option<u8>; 4] {
    let mut out = [None; 4];
    for (i, n) in (1u8..=4).enumerate() {
        if conn.write_all(&[DLE, EOT, n]).is_err() {
            break;
        }
        let _ = conn.flush();
        let reply = transport::read_until(conn, Duration::from_millis(600), |b| !b.is_empty());
        out[i] = reply.last().copied();
        // A printer that ignored the first query won't answer the rest.
        if i == 0 && out[0].is_none() {
            break;
        }
    }
    out
}

fn escpos_status(conn: &mut dyn Duplex) -> PrinterStatus {
    let [a, b, c, d] = query_escpos(conn);
    parse_escpos_status(a, b, c, d)
}

/// Status for a printer of the given language. Label printers (ZPL/TSPL)
/// must not receive ESC/POS bytes, so they only get a reachability check
/// (plus the spooler's view for Windows printers).
pub fn query_status_for(target: &PrintTarget, lang: crate::PrinterLanguage) -> PrinterStatus {
    if !lang.is_label() {
        return query_status(target);
    }
    transport::with_port_lock(&target.lock_key(), || match target {
        PrintTarget::Network { host, port } => match transport::open_tcp_duplex(host, *port) {
            Ok(_) => reachable_only("tcp", "Reachable; label printers don't report paper status"),
            Err(e) => offline(e),
        },
        // An open COM port says nothing about whether the printer is on.
        PrintTarget::Serial(port) => match transport::open_serial_duplex(port, &SerialSettings::default()) {
            Ok(_) => reachable_only("none", "Port is available; label printers don't report their status"),
            Err(e) => offline(e),
        },
        PrintTarget::WindowsPrinter(name) => spooler_only(name),
    })
}

fn reachable_only(source: &str, msg: &str) -> PrinterStatus {
    PrinterStatus {
        reachable: true,
        source: source.into(),
        messages: vec![msg.into()],
        ..Default::default()
    }
    .finish()
}

/// ESC/POS receipt printer status.
pub fn query_status(target: &PrintTarget) -> PrinterStatus {
    transport::with_port_lock(&target.lock_key(), || match target {
        PrintTarget::Serial(port) => serial_status(port),
        PrintTarget::Network { host, port } => match transport::open_tcp_duplex(host, *port) {
            Ok(mut conn) => {
                let mut s = escpos_status(conn.as_mut());
                if s.source == "none" {
                    // Reachable but no status support — report it as up.
                    s.source = "tcp".into();
                    s.messages.push("Reachable; printer does not report paper/cover status".into());
                    s = s.finish();
                }
                s
            }
            Err(e) => offline(e),
        },
        PrintTarget::WindowsPrinter(name) => spooler_status(name),
    })
}

fn offline(reason: String) -> PrinterStatus {
    PrinterStatus {
        reachable: false,
        source: "none".into(),
        messages: vec![reason],
        ..Default::default()
    }
    .finish()
}

fn serial_status(port: &str) -> PrinterStatus {
    match transport::open_serial_duplex(port, &SerialSettings::default()) {
        Ok(mut conn) => {
            let mut s = escpos_status(conn.as_mut());
            if s.source == "none" {
                s.messages.push(
                    "Port opened but the printer did not answer (it may be off, or not support status)".into(),
                );
            }
            s
        }
        Err(e) => offline(e),
    }
}

#[cfg(target_os = "windows")]
fn spooler_status(name: &str) -> PrinterStatus {
    // Printers on a real COM port can still answer ESC/POS status directly.
    let port_name = crate::windows_printer_port_name(name);
    if port_name.starts_with("COM") {
        let s = serial_status(port_name.trim_end_matches(':'));
        if s.source == "escpos" {
            return s;
        }
    }
    spooler_only(name)
}

#[cfg(target_os = "windows")]
fn spooler_only(name: &str) -> PrinterStatus {
    // Name as a WQL string literal inside a PowerShell double-quoted string
    // (UNC names like \\server\printer need their backslashes doubled).
    let wql = name
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('`', "``")
        .replace('$', "`$")
        .replace('"', "`\"");
    // Name inside a PowerShell single-quoted string.
    let ps = name.replace('\'', "''");
    // Works on PowerShell 2 (Windows 7) — no ConvertTo-Json.
    let script = format!(
        "$p = Get-WmiObject -Class Win32_Printer -Filter \"Name='{0}'\"; \
         if ($p) {{ $q = @(Get-WmiObject -Class Win32_PrintJob | Where-Object {{ $_.Name -like ('{1}' + ',*') }}).Count; \
         Write-Output (\"$($p.PrinterStatus)|$($p.DetectedErrorState)|$($p.WorkOffline)|$q\") }}",
        wql, ps
    );
    match crate::run_powershell(&script).filter(|s| !s.is_empty()) {
        Some(line) => parse_spooler_line(&line),
        None => offline(format!("Windows printer '{}' not found", name)),
    }
}

#[cfg(not(target_os = "windows"))]
fn spooler_status(_name: &str) -> PrinterStatus {
    offline("Windows printers are only supported on Windows".into())
}

#[cfg(not(target_os = "windows"))]
fn spooler_only(name: &str) -> PrinterStatus {
    spooler_status(name)
}

/// Parse "PrinterStatus|DetectedErrorState|WorkOffline|QueuedJobs" from WMI.
pub fn parse_spooler_line(line: &str) -> PrinterStatus {
    let parts: Vec<&str> = line.trim().split('|').collect();
    let num = |i: usize| parts.get(i).and_then(|v| v.trim().parse::<u32>().ok());
    let printer_status = num(0).unwrap_or(2);
    let error_state = num(1).unwrap_or(0);
    let work_offline = parts.get(2).map(|v| v.trim().eq_ignore_ascii_case("true")).unwrap_or(false);

    let mut s = PrinterStatus {
        reachable: true,
        source: "spooler".into(),
        online: Some(!work_offline && printer_status != 7 && error_state != 9),
        queued_jobs: num(3),
        ..Default::default()
    };
    match error_state {
        3 => s.paper_near_end = Some(true),
        4 => s.paper_out = Some(true),
        7 => s.cover_open = Some(true),
        8 | 10 => s.error = Some(true),
        _ => {}
    }
    if let Some(q) = s.queued_jobs.filter(|q| *q > 0) {
        s.messages.push(format!("{} job(s) waiting in the Windows print queue", q));
    }
    s.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_printer() {
        let s = parse_escpos_status(Some(0x16), Some(0x12), Some(0x12), Some(0x12));
        assert_eq!(s.state, "ok");
        assert_eq!(s.online, Some(true));
        assert_eq!(s.paper_out, Some(false));
        assert_eq!(s.drawer_pin_high, Some(true));
    }

    #[test]
    fn paper_out_and_cover_open() {
        // offline cause: cover open (bit2) + paper end stop (bit5)
        let s = parse_escpos_status(Some(0x1A), Some(0x12 | 0x04 | 0x20), Some(0x12), Some(0x12 | 0x60));
        assert_eq!(s.state, "error");
        assert_eq!(s.online, Some(false));
        assert_eq!(s.cover_open, Some(true));
        assert_eq!(s.paper_out, Some(true));
    }

    #[test]
    fn paper_low_is_warning() {
        let s = parse_escpos_status(Some(0x12), Some(0x12), Some(0x12), Some(0x12 | 0x0C));
        assert_eq!(s.state, "warning");
        assert_eq!(s.paper_near_end, Some(true));
    }

    #[test]
    fn no_answer_is_unknown_and_garbage_ignored() {
        assert_eq!(parse_escpos_status(None, None, None, None).state, "unknown");
        assert_eq!(parse_escpos_status(Some(0xFF), None, None, None).state, "unknown");
    }

    #[test]
    fn spooler_states() {
        assert_eq!(parse_spooler_line("3|2|False|0").state, "ok");
        assert_eq!(parse_spooler_line("3|4|False|0").state, "error");
        assert_eq!(parse_spooler_line("7|0|False|0").online, Some(false));
        assert_eq!(parse_spooler_line("3|0|True|2").queued_jobs, Some(2));
    }
}
