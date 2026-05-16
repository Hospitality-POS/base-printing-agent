use crate::job_queue::PrintJob;
use serialport::SerialPort;
use std::io::Write;
use std::time::Duration;

const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;
const LF: u8 = 0x0A;

#[derive(Debug, Clone)]
pub enum PrintTarget {
    Serial(String),
    Network { host: String, port: u16 },
}

impl PrintTarget {
    pub fn from_config(config: &crate::config::Config) -> Self {
        if config.printer_type == "network" {
            PrintTarget::Network {
                host: config.printer_host.clone().unwrap_or_default(),
                port: config.printer_network_port.unwrap_or(9100),
            }
        } else {
            PrintTarget::Serial(config.printer_port.clone())
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
    if port_name.starts_with("/dev/usb/") || port_name.starts_with("/dev/lp") {
        use std::fs::OpenOptions;
        let mut file = OpenOptions::new()
            .write(true)
            .open(port_name)
            .map_err(|e| format!("Failed to open device {}: {}", port_name, e))?;
        file.write_all(bytes)
            .map_err(|e| format!("Failed to write to device: {}", e))?;
    } else {
        let mut port = open_serial_port(port_name)?;
        port.write_all(bytes)
            .map_err(|e| format!("Failed to write to port: {}", e))?;
    }
    Ok(())
}

fn write_to_network(host: &str, port: u16, bytes: &[u8]) -> Result<(), String> {
    use std::net::TcpStream;
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("Cannot connect to printer at {} → {}", addr, e))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok();
    stream
        .write_all(bytes)
        .map_err(|e| format!("Failed to write to network printer {}: {}", addr, e))?;
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
                add_divider(&mut bytes);
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
