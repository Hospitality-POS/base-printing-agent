use crate::commands::PrintJob;
use serialport::SerialPort;
use std::io::Write;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use serialport::SerialPortType;

const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;
const LF: u8 = 0x0A;

#[derive(Debug, Clone)]
pub enum PrintTarget {
    Serial(String),
    Network { host: String, port: u16 },
}

impl PrintTarget {
    pub fn from_agent_config(agent: &crate::config::AgentConfig) -> Self {
        if agent.printer_type == "network" {
            PrintTarget::Network {
                host: agent.printer_host.clone().unwrap_or_default(),
                port: agent.printer_network_port.unwrap_or(9100) as u16,
            }
        } else {
            PrintTarget::Serial(agent.printer_port.clone())
        }
    }

    pub fn display(&self) -> String {
        match self {
            PrintTarget::Serial(p) => p.clone(),
            PrintTarget::Network { host, port } => format!("{}:{}", host, port),
        }
    }
}

pub fn get_available_ports() -> Vec<String> {
    match serialport::available_ports() {
        Ok(ports) => ports.into_iter().map(|p| p.port_name).collect(),
        Err(_) => vec![],
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PortInfo {
    pub name: String,
    pub port_type: PortType,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PortType {
    pub type_name: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

pub fn detect_thermal_printers() -> Vec<PortInfo> {
    let ports_info = match serialport::available_ports() {
        Ok(ports) => ports,
        Err(_) => return vec![],
    };

    ports_info.iter().map(|p| {
        let port_type = match &p.port_type {
            SerialPortType::UsbPort(info) => PortType {
                type_name: "USB".to_string(),
                vid: Some(info.vid),
                pid: Some(info.pid),
                manufacturer: info.manufacturer.clone(),
                product: info.product.clone(),
            },
            SerialPortType::PciPort => PortType {
                type_name: "PCI".to_string(),
                vid: None,
                pid: None,
                manufacturer: None,
                product: None,
            },
            SerialPortType::BluetoothPort => PortType {
                type_name: "Bluetooth".to_string(),
                vid: None,
                pid: None,
                manufacturer: None,
                product: None,
            },
            _ => PortType {
                type_name: "COM/Serial".to_string(),
                vid: None,
                pid: None,
                manufacturer: None,
                product: None,
            },
        };

        PortInfo {
            name: p.port_name.clone(),
            port_type,
        }
    }).collect()
}

fn get_local_ip() -> String {
    use std::net::UdpSocket;
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "192.168.1.1".to_string()
}

pub async fn discover_network_printers() -> Vec<String> {
    let local_ip = get_local_ip();
    let parts: Vec<&str> = local_ip.split('.').collect();
    if parts.len() != 4 {
        return vec![];
    }
    let base = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
    log::info!("Scanning subnet {}.1-254 on port 9100...", base);

    let mut handles = Vec::new();
    for i in 1u8..=254 {
        let ip = format!("{}.{}", base, i);
        let handle = tokio::spawn(async move {
            let addr = format!("{}:9100", ip);
            match tokio::time::timeout(
                Duration::from_millis(200),
                tokio::net::TcpStream::connect(&addr),
            )
            .await
            {
                Ok(Ok(_)) => Some(format!("{} (port 9100)", ip)),
                _ => None,
            }
        });
        handles.push(handle);
    }

    let mut found = Vec::new();
    for handle in handles {
        if let Ok(Some(entry)) = handle.await {
            found.push(entry);
        }
    }
    found.sort();
    log::info!("Network printer scan complete: {} found.", found.len());
    found
}

fn open_serial_port(port_name: &str) -> Result<Box<dyn SerialPort>, String> {
    serialport::new(port_name, 9600)
        .timeout(Duration::from_secs(5))
        .open()
        .map_err(|e| format!("Failed to open port {}: {}", port_name, e))
}

pub fn print_job(job: &PrintJob, target: &PrintTarget) -> Result<(), String> {
    let bytes = format_receipt(job);
    write_bytes(target, &bytes)
}

pub fn test_print(target: &PrintTarget) -> Result<(), String> {
    let mut bytes = Vec::new();
    initialize_printer(&mut bytes);
    add_text_bold(&mut bytes, "  ** TEST PRINT **\n");
    add_text(&mut bytes, "PrintAgent v0.1.0\n");
    add_divider(&mut bytes);
    add_text(&mut bytes, "Printer: ");
    add_text(&mut bytes, &target.display());
    add_text(&mut bytes, "\n");
    add_text(&mut bytes, "Status: OK\n");
    add_divider(&mut bytes);
    add_feed(&mut bytes, 3);
    cut_paper(&mut bytes);
    write_bytes(target, &bytes)
}

fn write_bytes(target: &PrintTarget, bytes: &[u8]) -> Result<(), String> {
    match target {
        PrintTarget::Serial(port_name) => write_to_serial(port_name, bytes),
        PrintTarget::Network { host, port } => write_to_network(host, *port, bytes),
    }
}

fn write_to_serial(port_name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut port = open_serial_port(port_name)?;
    port.write_all(bytes)
        .map_err(|e| format!("Failed to write to serial port: {}", e))?;
    Ok(())
}

fn write_to_network(host: &str, port: u16, bytes: &[u8]) -> Result<(), String> {
    use std::net::TcpStream;
    use std::io::Write;
    
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("Failed to connect to network printer: {}", e))?;
    
    stream.write_all(bytes)
        .map_err(|e| format!("Failed to write to network printer: {}", e))?;
    Ok(())
}

pub fn format_receipt(job: &PrintJob) -> Vec<u8> {
    let mut bytes = Vec::new();

    initialize_printer(&mut bytes);

    for line in &job.lines {
        match line.line_type.as_str() {
            "header" => {
                set_alignment_center(&mut bytes);
                set_text_double_height(&mut bytes);
                add_text_bold(&mut bytes, &line.text);
                bytes.push(LF);
                set_text_normal(&mut bytes);
                set_alignment_left(&mut bytes);
            }
            "item" => {
                set_alignment_left(&mut bytes);
                let price = line.price.as_deref().unwrap_or("");
                let qty_str = line
                    .qty
                    .map(|q| format!("{}x ", q))
                    .unwrap_or_default();
                let item_text = format!("{}{}", qty_str, line.text);
                let padded = format_two_columns(&item_text, price, 42);
                add_text(&mut bytes, &padded);
                bytes.push(LF);
            }
            "divider" => {
                let divider_text = if line.text.is_empty() {
                    "-".repeat(42)
                } else {
                    line.text.clone()
                };
                add_text(&mut bytes, &divider_text);
                bytes.push(LF);
            }
            "total" => {
                add_divider(&mut bytes);
                set_alignment_right(&mut bytes);
                set_text_double_height(&mut bytes);
                add_text_bold(&mut bytes, &line.text);
                bytes.push(LF);
                set_text_normal(&mut bytes);
                set_alignment_left(&mut bytes);
            }
            "footer" => {
                add_feed(&mut bytes, 1);
                set_alignment_center(&mut bytes);
                add_text(&mut bytes, &line.text);
                bytes.push(LF);
                set_alignment_left(&mut bytes);
            }
            _ => {
                add_text(&mut bytes, &line.text);
                bytes.push(LF);
            }
        }
    }

    add_feed(&mut bytes, 3);

    if job.cut_paper {
        cut_paper(&mut bytes);
    }

    bytes
}

fn format_two_columns(left: &str, right: &str, width: usize) -> String {
    if left.len() + right.len() + 1 > width {
        format!("{}\n{:>width$}", left, right, width = width)
    } else {
        let spaces = width - left.len() - right.len();
        format!("{}{}{}", left, " ".repeat(spaces), right)
    }
}

fn initialize_printer(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[ESC, b'@']);
}

fn set_alignment_left(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[ESC, b'a', 0]);
}

fn set_alignment_center(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[ESC, b'a', 1]);
}

fn set_alignment_right(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[ESC, b'a', 2]);
}

fn set_text_double_height(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[GS, b'!', 0x11]);
}

fn set_text_normal(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[GS, b'!', 0x00]);
}

fn add_text_bold(buf: &mut Vec<u8>, text: &str) {
    buf.extend_from_slice(&[ESC, b'E', 1]);
    buf.extend_from_slice(text.as_bytes());
    buf.extend_from_slice(&[ESC, b'E', 0]);
}

fn add_text(buf: &mut Vec<u8>, text: &str) {
    buf.extend_from_slice(text.as_bytes());
}

fn add_divider(buf: &mut Vec<u8>) {
    let line = "-".repeat(42);
    buf.extend_from_slice(line.as_bytes());
    buf.push(LF);
}

fn add_feed(buf: &mut Vec<u8>, lines: u8) {
    buf.extend_from_slice(&[ESC, b'd', lines]);
}

fn cut_paper(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[GS, b'V', 0]);
}
