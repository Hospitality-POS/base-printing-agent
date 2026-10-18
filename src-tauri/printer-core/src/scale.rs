//! Weighing scales over serial (RS-232 / USB-serial) or TCP.
//!
//! Supported protocols:
//! - `toledo`  — Mettler Toledo 8217 style: host sends `W`, scale replies
//!               `STX weight CR`, or `STX ? status CR` while unstable.
//!               Also used by many Dibal / ACLAS / CAS models in "Toledo" mode.
//! - `nci`     — NCI / Avery Weigh-Tronix / Brecknell / Salter: host sends
//!               `W CR`, scale replies `LF weight unit CR LF status CR ETX`.
//! - `cas`     — CAS continuous stream: `ST,GS,+  1.234kg` (ST=stable,
//!               US=unstable, OL=overload), no request needed.
//! - `generic` — continuous or polled (optional `requestCommand`); takes the
//!               last complete line containing a number.

use crate::transport::{self, Duplex, SerialSettings};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const STX: u8 = 0x02;
const ETX: u8 = 0x03;
const CR: u8 = b'\r';
const LF: u8 = b'\n';

fn default_connection() -> String {
    "serial".into()
}
fn default_baud() -> u32 {
    9600
}
fn default_data_bits() -> u8 {
    8
}
fn default_parity() -> String {
    "none".into()
}
fn default_stop_bits() -> u8 {
    1
}
fn default_protocol() -> String {
    "generic".into()
}
fn default_unit() -> String {
    "kg".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// "serial" or "network".
    #[serde(default = "default_connection")]
    pub connection: String,
    /// COM port / device path for serial scales.
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub network_port: Option<u16>,
    #[serde(default = "default_baud")]
    pub baud_rate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default = "default_parity")]
    pub parity: String,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    /// "toledo" | "nci" | "cas" | "generic".
    #[serde(default = "default_protocol")]
    pub protocol: String,
    /// Polling command for `generic` scales, e.g. "W\r" (supports \r \n \xNN).
    #[serde(default)]
    pub request_command: Option<String>,
    #[serde(default)]
    pub tare_command: Option<String>,
    #[serde(default)]
    pub zero_command: Option<String>,
    /// Implied decimal places when the scale sends digits without a point
    /// (e.g. "01234" with 3 decimals = 1.234).
    #[serde(default)]
    pub decimals: Option<u8>,
    /// Unit to assume when the scale doesn't send one.
    #[serde(default = "default_unit")]
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightReading {
    /// `None` while the scale only reports a status (e.g. in motion).
    pub weight: Option<f64>,
    pub unit: String,
    pub stable: bool,
    pub overload: bool,
    pub under_zero: bool,
    /// The frame we parsed, for troubleshooting.
    pub raw: String,
}

/// Turn "W\r" / "\x05" style escapes into bytes.
pub fn unescape_command(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'r' => {
                    out.push(CR);
                    i += 2;
                    continue;
                }
                b'n' => {
                    out.push(LF);
                    i += 2;
                    continue;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                    continue;
                }
                b'x' => {
                    if let Some(Ok(v)) = s.get(i + 2..i + 4).map(|h| u8::from_str_radix(h, 16)) {
                        out.push(v);
                        i += 4;
                        continue;
                    }
                }
                _ => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn open(cfg: &ScaleConfig) -> Result<Box<dyn Duplex>, String> {
    if cfg.connection == "network" {
        let host = cfg.host.as_deref().unwrap_or("").trim();
        if host.is_empty() {
            return Err("Network scale needs a host/IP address".into());
        }
        transport::open_tcp_duplex(host, cfg.network_port.unwrap_or(4001))
    } else {
        if cfg.port.trim().is_empty() {
            return Err("Scale needs a COM port".into());
        }
        transport::open_serial_duplex(
            cfg.port.trim(),
            &SerialSettings {
                baud_rate: cfg.baud_rate,
                data_bits: cfg.data_bits,
                parity: cfg.parity.clone(),
                stop_bits: cfg.stop_bits,
            },
        )
    }
}

fn lock_key(cfg: &ScaleConfig) -> String {
    if cfg.connection == "network" {
        format!("{}:{}", cfg.host.as_deref().unwrap_or(""), cfg.network_port.unwrap_or(4001))
    } else {
        cfg.port.clone()
    }
}

fn count_terminators(b: &[u8]) -> usize {
    b.iter().filter(|c| matches!(**c, CR | LF | ETX)).count()
}

/// Read one weight from the scale.
pub fn read_weight(cfg: &ScaleConfig) -> Result<WeightReading, String> {
    transport::with_port_lock(&lock_key(cfg), || {
        let mut conn = open(cfg)?;
        let conn = conn.as_mut();
        let send = |conn: &mut dyn Duplex, bytes: &[u8]| -> Result<(), String> {
            conn.write_all(bytes)
                .and_then(|_| conn.flush())
                .map_err(|e| format!("Failed to send request to scale: {}", e))
        };

        let raw = match cfg.protocol.as_str() {
            "toledo" => {
                send(conn, b"W")?;
                transport::read_until(conn, Duration::from_millis(1500), |b| {
                    b.contains(&STX) && b.iter().rposition(|c| *c == CR) > b.iter().position(|c| *c == STX)
                })
            }
            "nci" => {
                send(conn, b"W\r")?;
                transport::read_until(conn, Duration::from_millis(1500), |b| b.contains(&ETX))
            }
            "cas" => transport::read_until(conn, Duration::from_millis(1500), |b| count_terminators(b) >= 3),
            _ => match cfg.request_command.as_deref().filter(|c| !c.is_empty()) {
                Some(cmd) => {
                    send(conn, &unescape_command(cmd))?;
                    transport::read_until(conn, Duration::from_millis(1500), |b| {
                        count_terminators(b) >= 1 && b.iter().any(|c| c.is_ascii_digit())
                    })
                }
                None => transport::read_until(conn, Duration::from_millis(1500), |b| count_terminators(b) >= 3),
            },
        };

        if raw.is_empty() {
            return Err("No data from scale — check the port, cable, baud rate and protocol".into());
        }
        parse_reading(cfg, &raw)
    })
}

/// Send tare / zero to the scale (protocols that support it).
pub fn send_command(cfg: &ScaleConfig, command: &str) -> Result<(), String> {
    let bytes: Vec<u8> = match (cfg.protocol.as_str(), command) {
        ("toledo", "tare") => b"T".to_vec(),
        ("toledo", "zero") => b"Z".to_vec(),
        ("nci", "tare") => b"T\r".to_vec(),
        ("nci", "zero") => b"Z\r".to_vec(),
        (_, "tare") => cfg.tare_command.as_deref().map(unescape_command).unwrap_or_default(),
        (_, "zero") => cfg.zero_command.as_deref().map(unescape_command).unwrap_or_default(),
        (_, other) => return Err(format!("Unknown scale command '{}'", other)),
    };
    if bytes.is_empty() {
        return Err(format!(
            "This scale's protocol has no '{}' command — set one in the scale settings",
            command
        ));
    }
    transport::with_port_lock(&lock_key(cfg), || {
        let mut conn = open(cfg)?;
        conn.write_all(&bytes)
            .and_then(|_| conn.flush())
            .map_err(|e| format!("Failed to send {} to scale: {}", command, e))
    })
}

/// Split a buffer into complete frames (text between terminators). The tail
/// after the last terminator is incomplete and dropped.
fn complete_frames(raw: &[u8]) -> Vec<String> {
    let mut frames = Vec::new();
    let mut cur = Vec::new();
    for &b in raw {
        match b {
            CR | LF | ETX => {
                let s = String::from_utf8_lossy(&cur).trim().to_string();
                if !s.is_empty() {
                    frames.push(s);
                }
                cur.clear();
            }
            STX => cur.clear(),
            _ => cur.push(b),
        }
    }
    frames
}

/// Find the first number in `s` and the unit right after it.
fn parse_number_unit(s: &str, decimals: Option<u8>) -> Option<(f64, Option<String>)> {
    let chars: Vec<char> = s.chars().collect();
    let start = chars.iter().position(|c| c.is_ascii_digit())?;
    let mut end = start;
    let mut seen_point = false;
    while end < chars.len() {
        let c = chars[end];
        if c.is_ascii_digit() {
            end += 1;
        } else if (c == '.' || c == ',') && !seen_point && end + 1 < chars.len() && chars[end + 1].is_ascii_digit() {
            seen_point = true;
            end += 1;
        } else {
            break;
        }
    }
    let digits: String = chars[start..end].iter().map(|c| if *c == ',' { '.' } else { *c }).collect();
    let mut value: f64 = digits.parse().ok()?;
    if !seen_point {
        if let Some(d) = decimals.filter(|d| *d > 0) {
            value /= 10f64.powi(d as i32);
        }
    }
    // Sign: a '-' before the digits, possibly separated by spaces.
    let negative = chars[..start]
        .iter()
        .rev()
        .find(|c| **c != ' ')
        .map(|c| *c == '-')
        .unwrap_or(false);
    if negative {
        value = -value;
    }

    let unit: String = chars[end..]
        .iter()
        .skip_while(|c| **c == ' ')
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_lowercase();
    let unit = match unit.as_str() {
        "kg" | "g" | "oz" => Some(unit),
        "lb" | "lbs" => Some("lb".to_string()),
        _ => None,
    };
    Some((value, unit))
}

pub fn parse_reading(cfg: &ScaleConfig, raw: &[u8]) -> Result<WeightReading, String> {
    let frames = complete_frames(raw);
    let raw_text = String::from_utf8_lossy(raw)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string();

    let mut reading = WeightReading {
        weight: None,
        unit: cfg.unit.clone(),
        stable: true,
        overload: false,
        under_zero: false,
        raw: raw_text,
    };

    match cfg.protocol.as_str() {
        "toledo" => {
            let frame = frames.last().ok_or("Incomplete reply from scale")?;
            if let Some(rest) = frame.strip_prefix('?') {
                // Status byte: bit0 motion, bit1 over capacity, bit2 under zero.
                let status = rest.bytes().next().unwrap_or(0);
                reading.stable = status & 0x01 == 0;
                reading.overload = status & 0x02 != 0;
                reading.under_zero = status & 0x04 != 0;
                if status & 0x07 == 0 {
                    reading.stable = false;
                }
            } else {
                let (w, unit) = parse_number_unit(frame, cfg.decimals).ok_or("No weight in scale reply")?;
                reading.weight = Some(w);
                if let Some(u) = unit {
                    reading.unit = u;
                }
            }
        }
        "nci" => {
            if let Some(f) = frames.iter().find(|f| f.chars().any(|c| c.is_ascii_digit()) && !f.starts_with('S')) {
                if let Some((w, unit)) = parse_number_unit(f, cfg.decimals) {
                    reading.weight = Some(w);
                    if let Some(u) = unit {
                        reading.unit = u;
                    }
                }
            }
            // Status line: "S" + 2 bytes. Byte 1 bit0 = motion; byte 2 bit1 = over capacity, bit0 = under capacity.
            if let Some(st) = frames.iter().rev().find(|f| f.starts_with('S') && f.len() >= 3) {
                let b = st.as_bytes();
                reading.stable = b[1] & 0x01 == 0;
                reading.under_zero = b[2] & 0x01 != 0;
                reading.overload = b[2] & 0x02 != 0;
            }
            if frames.iter().any(|f| f.starts_with('?')) {
                reading.stable = false;
            }
            if reading.weight.is_none() && !reading.overload && !reading.under_zero && reading.stable {
                return Err("No weight in scale reply".into());
            }
        }
        _ => {
            // cas / generic: last complete frame that has a number (or an
            // overload marker).
            let frame = frames
                .iter()
                .rev()
                .find(|f| f.chars().any(|c| c.is_ascii_digit()) || f.to_uppercase().starts_with("OL"))
                .ok_or("No weight in scale data — check baud rate / protocol")?;
            let upper = frame.to_uppercase();
            reading.overload = upper.starts_with("OL") || upper.contains("OVER");
            reading.stable = !(upper.starts_with("US") || upper.contains(",US") || frame.contains('?') || upper.contains("MOTION"));
            if let Some((w, unit)) = parse_number_unit(frame, cfg.decimals) {
                reading.weight = Some(w);
                if let Some(u) = unit {
                    reading.unit = u;
                }
            }
        }
    }

    if let Some(w) = reading.weight {
        if w < 0.0 {
            reading.under_zero = true;
        }
    }
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(protocol: &str) -> ScaleConfig {
        serde_json::from_value(serde_json::json!({ "id": "s1", "protocol": protocol })).unwrap()
    }

    #[test]
    fn toledo_stable_weight() {
        let r = parse_reading(&cfg("toledo"), b"\x0201.234\r").unwrap();
        assert_eq!(r.weight, Some(1.234));
        assert!(r.stable);
        assert_eq!(r.unit, "kg");
    }

    #[test]
    fn toledo_implied_decimals_and_motion() {
        let mut c = cfg("toledo");
        c.decimals = Some(3);
        assert_eq!(parse_reading(&c, b"\x0201234\r").unwrap().weight, Some(1.234));
        let r = parse_reading(&c, b"\x02?\x01\r").unwrap();
        assert_eq!(r.weight, None);
        assert!(!r.stable);
    }

    #[test]
    fn cas_continuous_takes_last_complete_frame() {
        let data = b"kg\r\nUS,GS,+  0.500kg\r\nST,GS,+  0.750kg\r\nST,GS,+  0.7";
        let r = parse_reading(&cfg("cas"), data).unwrap();
        assert_eq!(r.weight, Some(0.75));
        assert!(r.stable);
        let r = parse_reading(&cfg("cas"), b"US,GS,-  0.020kg\r\n").unwrap();
        assert_eq!(r.weight, Some(-0.02));
        assert!(!r.stable);
        assert!(r.under_zero);
        assert!(parse_reading(&cfg("cas"), b"OL,GS,+ ------kg\r\n").unwrap().overload);
    }

    #[test]
    fn nci_reply() {
        let r = parse_reading(&cfg("nci"), b"\n  2.50lb\r\n\nS00\r\x03").unwrap();
        assert_eq!(r.weight, Some(2.5));
        assert_eq!(r.unit, "lb");
        assert!(r.stable);
        let r = parse_reading(&cfg("nci"), b"\n  2.50lb\r\n\nS10\r\x03").unwrap();
        assert!(!r.stable);
    }

    #[test]
    fn generic_grams_with_comma() {
        let r = parse_reading(&cfg("generic"), b"N.W. 1250,5 g\r\n").unwrap();
        assert_eq!(r.weight, Some(1250.5));
        assert_eq!(r.unit, "g");
    }

    #[test]
    fn unescape() {
        assert_eq!(unescape_command("W\\r"), b"W\r".to_vec());
        assert_eq!(unescape_command("\\x05"), vec![0x05]);
        assert_eq!(unescape_command("P\\n"), b"P\n".to_vec());
    }
}
