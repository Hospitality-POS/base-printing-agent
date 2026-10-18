use serialport::SerialPort;
use std::io::Write;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use serialport::SerialPortType;

pub mod label;
pub mod scale;
pub mod status;
pub mod transport;

pub use label::{LabelElement, LabelJob, PrinterLanguage};
pub use scale::{ScaleConfig, WeightReading};
pub use status::PrinterStatus;

const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;
const LF: u8 = 0x0A;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobLine {
    #[serde(rename = "type")]
    pub line_type: String,
    pub text: String,
    pub qty: Option<u32>,
    pub price: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    #[serde(rename = "_id")]
    pub id: Option<String>,
    pub job_id: String,
    pub branch_id: String,
    pub destination_zone: String,
    pub content_type: String,
    pub lines: Vec<PrintJobLine>,
    pub cut_paper: bool,
    pub priority: String,
    pub created_at: String,
    #[serde(rename = "main_category_id", default)]
    pub main_category_id: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(rename = "location_id", default)]
    pub location_id: Option<String>,
    #[serde(rename = "location_name", default)]
    pub location_name: Option<String>,
    #[serde(rename = "table_name", default)]
    pub table_name: Option<String>,
    /// Kick the cash drawer connected to this (receipt) printer.
    #[serde(default, alias = "open_drawer")]
    pub open_drawer: bool,
    /// Label layout, used when the job goes to a ZPL/TSPL label printer.
    #[serde(default)]
    pub label: Option<LabelJob>,
}

#[derive(Debug, Clone)]
pub enum PrintTarget {
    Serial(String),
    Network { host: String, port: u16 },
    WindowsPrinter(String),
}

impl PrintTarget {
    pub fn from_fields(
        printer_type: &str,
        printer_port: &str,
        printer_host: Option<&str>,
        printer_network_port: Option<i64>,
    ) -> Self {
        match printer_type {
            "network" => PrintTarget::Network {
                host: printer_host.unwrap_or_default().to_string(),
                port: printer_network_port.unwrap_or(9100) as u16,
            },
            "windows" => PrintTarget::WindowsPrinter(printer_port.to_string()),
            _ => PrintTarget::Serial(printer_port.to_string()),
        }
    }

    /// Key used to serialise access to the same physical device.
    pub fn lock_key(&self) -> String {
        match self {
            PrintTarget::Serial(p) => p.clone(),
            PrintTarget::Network { host, port } => format!("{}:{}", host, port),
            PrintTarget::WindowsPrinter(name) => format!("winprinter:{}", name),
        }
    }

    pub fn display(&self) -> String {
        match self {
            PrintTarget::Serial(p) => p.clone(),
            PrintTarget::Network { host, port } => format!("{}:{}", host, port),
            PrintTarget::WindowsPrinter(name) => format!("WinPrinter:{}", name),
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

// ---------------------------------------------------------------------------
// OS / compatibility detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    pub name: String,
    /// True when the OS supports the Tauri/WebView2 desktop app (Windows 10+).
    pub supports_desktop_app: bool,
    /// True when the legacy helper binary is expected to work (Windows 7+).
    pub supports_helper: bool,
    /// Machine hostname (e.g. COMPUTERNAME on Windows), if available.
    pub hostname: Option<String>,
}

fn windows_version_name(major: u32, minor: u32, build: u32) -> String {
    match (major, minor) {
        (10, 0) if build >= 22000 => "Windows 11".to_string(),
        (10, 0) => "Windows 10".to_string(),
        (6, 3) => "Windows 8.1".to_string(),
        (6, 2) => "Windows 8".to_string(),
        (6, 1) => "Windows 7".to_string(),
        (6, 0) => "Windows Vista".to_string(),
        _ => format!("Windows ({}.{})", major, minor),
    }
}

/// Detect the operating system version.
/// On Windows this parses `cmd /c ver`, which reports the true kernel version
/// (e.g. "Microsoft Windows [Version 10.0.19045.3803]") on every release from
/// Windows XP through Windows 11 without needing extra crates or APIs.
fn machine_hostname() -> Option<String> {
    if let Ok(name) = std::env::var("COMPUTERNAME") {
        if !name.is_empty() {
            return Some(name);
        }
    }
    std::env::var("HOSTNAME").ok().filter(|n| !n.is_empty())
}

pub fn get_os_info() -> OsInfo {
    let hostname = machine_hostname();
    #[cfg(target_os = "windows")]
    {
        if let Some((major, minor, build)) = windows_kernel_version() {
            return OsInfo {
                major,
                minor,
                build,
                name: windows_version_name(major, minor, build),
                supports_desktop_app: major >= 10,
                supports_helper: major > 6 || (major == 6 && minor >= 1),
                hostname,
            };
        }
        // Could not determine version — assume modern Windows
        return OsInfo {
            major: 10,
            minor: 0,
            build: 0,
            name: "Windows (unknown version)".to_string(),
            supports_desktop_app: true,
            supports_helper: true,
            hostname,
        };
    }
    #[cfg(not(target_os = "windows"))]
    {
        OsInfo {
            major: 0,
            minor: 0,
            build: 0,
            name: std::env::consts::OS.to_string(),
            supports_desktop_app: true,
            supports_helper: true,
            hostname,
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_kernel_version() -> Option<(u32, u32, u32)> {
    use std::process::Command;
    let output = Command::new("cmd").args(["/c", "ver"]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // Expected: "Microsoft Windows [Version 10.0.19045.3803]"
    let start = text.find("[Version ")? + "[Version ".len();
    let end = text[start..].find(']')? + start;
    let version = &text[start..end];
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let build = parts.next().unwrap_or("0").parse().unwrap_or(0);
    Some((major, minor, build))
}

/// Detect the installed Windows PowerShell major version (0 if none/error).
/// PowerShell 2.0 ships with Windows 7; 5.1 with Windows 10/11.
#[cfg(target_os = "windows")]
pub fn powershell_major_version() -> u32 {
    use std::process::Command;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Write-Output $PSVersionTable.PSVersion.Major",
        ])
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0),
        Err(_) => 0,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn powershell_major_version() -> u32 {
    0
}

/// True when the newer `Get-Printer`/`Get-PrinterPort` cmdlets are available
/// (PrintManagement module: Windows 8+ / PowerShell 3+).
#[cfg(target_os = "windows")]
fn has_print_management_cmdlets() -> bool {
    let os = get_os_info();
    (os.major > 6 || (os.major == 6 && os.minor >= 2)) && powershell_major_version() >= 3
}

/// Human-readable compatibility report, used by `helper --check`.
pub fn compatibility_report() -> String {
    let os = get_os_info();
    let mut lines = vec![
        format!("OS: {} ({}.{}.{})", os.name, os.major, os.minor, os.build),
        format!("Desktop app (WebView2) supported: {}", os.supports_desktop_app),
        format!("Helper agent supported: {}", os.supports_helper),
    ];
    #[cfg(target_os = "windows")]
    {
        lines.push(format!(
            "PowerShell major version: {}",
            powershell_major_version()
        ));
        lines.push(format!(
            "Get-Printer cmdlets available: {}",
            has_print_management_cmdlets()
        ));
    }
    lines.push(format!(
        "Serial ports: {:?}",
        get_available_ports()
    ));
    lines.push(format!("Windows printers: {:?}", list_windows_printers()));
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Windows printer enumeration (version-aware)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn run_powershell(script: &str) -> Option<String> {
    use std::process::Command;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(text)
}

pub fn list_windows_printers() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        // Get-Printer requires the PrintManagement module (Windows 8+ / PS3+).
        // On Windows 7 / PowerShell 2 fall back to WMI, which exists everywhere.
        let output = if has_print_management_cmdlets() {
            run_powershell("Get-Printer | Select-Object -ExpandProperty Name")
        } else {
            None
        };

        let output = output.or_else(|| {
            run_powershell("Get-WmiObject -Class Win32_Printer | Select-Object -ExpandProperty Name")
        });

        return output
            .map(|text| {
                text.lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default();
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![]
    }
}

/// Look up the port name a Windows printer is attached to (COM1, USB001, ...).
#[cfg(target_os = "windows")]
fn windows_printer_port_name(printer_name: &str) -> String {
    let escaped = printer_name.replace('\'', "''");
    let output = if has_print_management_cmdlets() {
        run_powershell(&format!(
            "(Get-Printer -Name '{}' -ErrorAction SilentlyContinue).PortName",
            escaped
        ))
    } else {
        None
    };

    output
        .or_else(|| {
            run_powershell(&format!(
                "(Get-WmiObject -Class Win32_Printer -Filter \"Name='{}'\").PortName",
                escaped.replace('\'', "\\'")
            ))
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
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

/// Scan the local /24 subnet for devices listening on RAW printing port 9100.
/// Synchronous implementation (std threads) so it works in the helper binary
/// without an async runtime, and on older Windows versions.
pub fn discover_network_printers() -> Vec<String> {
    let local_ip = get_local_ip();
    let parts: Vec<&str> = local_ip.split('.').collect();
    if parts.len() != 4 {
        return vec![];
    }
    let base = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
    log::info!("Scanning subnet {}.1-254 on port 9100...", base);

    let mut found: Vec<String> = Vec::new();
    let mut handles = Vec::with_capacity(254);
    for i in 1u8..=254 {
        let addr = format!("{}.{}:9100", base, i);
        handles.push(std::thread::spawn(move || {
            let sock_addr: std::net::SocketAddr = addr.parse().ok()?;
            match std::net::TcpStream::connect_timeout(&sock_addr, Duration::from_millis(200)) {
                Ok(_) => Some(format!("{} (port 9100)", sock_addr.ip())),
                Err(_) => None,
            }
        }));
    }
    for handle in handles {
        if let Ok(Some(entry)) = handle.join() {
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

// ---------------------------------------------------------------------------
// Cash drawer
// ---------------------------------------------------------------------------

/// Which drawer-kick connector pin to pulse. Most drawers use pin 2; some
/// printers wire a second drawer to pin 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawerPin {
    #[default]
    Pin2,
    Pin5,
    Both,
}

impl DrawerPin {
    pub fn parse(s: Option<&str>) -> Self {
        match s.unwrap_or("").trim() {
            "5" | "pin5" => DrawerPin::Pin5,
            "both" => DrawerPin::Both,
            _ => DrawerPin::Pin2,
        }
    }
}

/// ESC p m t1 t2 — pulse on for 50 ms, off for 500 ms.
pub fn drawer_kick_bytes(pin: DrawerPin) -> Vec<u8> {
    let pulse = |m: u8| [ESC, b'p', m, 25, 250];
    match pin {
        DrawerPin::Pin2 => pulse(0).to_vec(),
        DrawerPin::Pin5 => pulse(1).to_vec(),
        DrawerPin::Both => [pulse(0), pulse(1)].concat(),
    }
}

/// Open the cash drawer wired to a receipt printer's drawer-kick port.
pub fn open_cash_drawer(target: &PrintTarget, pin: DrawerPin) -> Result<(), String> {
    write_bytes(target, &drawer_kick_bytes(pin))
}

// ---------------------------------------------------------------------------
// Printing
// ---------------------------------------------------------------------------

/// Print a receipt-style job as ESC/POS (back-compat entry point).
pub fn print_job(job: &PrintJob, target: &PrintTarget) -> Result<(), String> {
    print_job_as(job, target, PrinterLanguage::EscPos, DrawerPin::default())
}

/// Render the job in the printer's language and send it.
pub fn print_job_as(
    job: &PrintJob,
    target: &PrintTarget,
    lang: PrinterLanguage,
    drawer_pin: DrawerPin,
) -> Result<(), String> {
    write_bytes(target, &render_job(job, lang, drawer_pin))
}

pub fn render_job(job: &PrintJob, lang: PrinterLanguage, drawer_pin: DrawerPin) -> Vec<u8> {
    if lang.is_label() {
        if job.open_drawer {
            log::warn!("Job {} asks to open a drawer, but this is a label printer; ignoring.", job.job_id);
        }
        let label = job.label.clone().unwrap_or_else(|| {
            LabelJob::from_lines(&job.lines.iter().map(label_line_text).collect::<Vec<_>>())
        });
        return label.render(lang);
    }

    let mut bytes = format_receipt(job);
    if job.open_drawer {
        // Right after ESC @ so the drawer opens while the receipt prints.
        bytes.splice(2..2, drawer_kick_bytes(drawer_pin));
    }
    bytes
}

fn label_line_text(line: &PrintJobLine) -> String {
    match (line.qty, line.price.as_deref()) {
        (Some(q), Some(p)) => format!("{}x {}  {}", q, line.text, p),
        (Some(q), None) => format!("{}x {}", q, line.text),
        (None, Some(p)) => format!("{}  {}", line.text, p),
        _ => line.text.clone(),
    }
}

pub fn print_label(label: &LabelJob, target: &PrintTarget, lang: PrinterLanguage) -> Result<(), String> {
    if !lang.is_label() {
        return Err("This printer is in receipt mode (ESC/POS); set its language to ZPL or TSPL to print labels".into());
    }
    write_bytes(target, &label.render(lang))
}

/// Test page in the printer's own language.
pub fn test_print_as(target: &PrintTarget, lang: PrinterLanguage) -> Result<(), String> {
    if lang.is_label() {
        return write_bytes(target, &LabelJob::test_label(&target.display()).render(lang));
    }
    test_print(target)
}

pub fn test_print(target: &PrintTarget) -> Result<(), String> {
    let mut bytes = Vec::new();
    initialize_printer(&mut bytes);
    add_text_bold(&mut bytes, "  ** TEST PRINT **\n");
    add_text(&mut bytes, concat!("PrintAgent v", env!("CARGO_PKG_VERSION"), "\n"));
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

pub fn write_bytes(target: &PrintTarget, bytes: &[u8]) -> Result<(), String> {
    transport::with_port_lock(&target.lock_key(), || match target {
        PrintTarget::Serial(port_name) => write_to_serial(port_name, bytes),
        PrintTarget::Network { host, port } => write_to_network(host, *port, bytes),
        PrintTarget::WindowsPrinter(name) => write_to_windows_printer(name, bytes),
    })
}

fn write_to_serial(port_name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut port = open_serial_port(port_name)?;
    port.write_all(bytes)
        .map_err(|e| format!("Failed to write to serial port: {}", e))?;
    Ok(())
}

fn write_to_network(host: &str, port: u16, bytes: &[u8]) -> Result<(), String> {
    use std::net::TcpStream;

    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("Failed to connect to network printer: {}", e))?;

    stream.write_all(bytes)
        .map_err(|e| format!("Failed to write to network printer: {}", e))?;
    Ok(())
}

fn write_to_windows_printer(printer_name: &str, bytes: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::fs;

        // Write ESC/POS bytes to temp file
        let temp_path = std::env::temp_dir().join("print_agent_job.bin");
        fs::write(&temp_path, bytes)
            .map_err(|e| format!("Failed to write temp print file: {}", e))?;

        let temp_str = temp_path.display().to_string();

        // Step 1: Try to get the printer's physical port (COM1, USB001, LPT1 etc.)
        let port_name = windows_printer_port_name(printer_name);

        log::info!("Printer '{}' uses port: '{}'", printer_name, port_name);

        // Step 2: If it's a physical USB/COM/LPT port, write directly to it
        let result = if !port_name.is_empty()
            && (port_name.starts_with("COM")
                || port_name.starts_with("USB")
                || port_name.starts_with("LPT"))
        {
            log::info!("Writing directly to physical port: {}", port_name);
            let port_path = format!("\\\\.\\{}", port_name);
            match fs::OpenOptions::new().write(true).open(&port_path) {
                Ok(mut f) => f
                    .write_all(bytes)
                    .map_err(|e| format!("Failed to write to port {}: {}", port_name, e)),
                Err(e) => {
                    log::warn!("Direct port write failed: {}, falling back to WinSpool", e);
                    // Fall through to WinSpool API below
                    winspool_raw_print(printer_name, &temp_str)
                }
            }
        } else {
            // Step 3: Use Windows Spooler API via PowerShell P/Invoke for all other cases
            // (USB001 virtual ports, network printers, PDF printers etc.)
            winspool_raw_print(printer_name, &temp_str)
        };

        let _ = fs::remove_file(&temp_path);
        result
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Windows printer not supported on this platform".to_string())
    }
}

#[cfg(target_os = "windows")]
fn winspool_raw_print(printer_name: &str, temp_file_path: &str) -> Result<(), String> {
    use std::process::Command;

    // Use Windows Spooler API (winspool.drv) via PowerShell Add-Type P/Invoke
    // This is the correct way to send raw ESC/POS bytes to any Windows printer.
    // Add-Type works on PowerShell 2.0 (Windows 7) through 5.1/7.x.
    let script = format!(
        r#"
$bytes = [System.IO.File]::ReadAllBytes('{temp}');
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class RawPrinterHelper {{
    [DllImport("winspool.drv", CharSet=CharSet.Auto, SetLastError=true)]
    public static extern bool OpenPrinter(string pPrinterName, out IntPtr phPrinter, IntPtr pDefault);
    [DllImport("winspool.drv", CharSet=CharSet.Auto, SetLastError=true)]
    public static extern bool ClosePrinter(IntPtr hPrinter);
    [DllImport("winspool.drv", CharSet=CharSet.Auto, SetLastError=true)]
    public static extern int StartDocPrinter(IntPtr hPrinter, int Level, ref DOCINFO pDocInfo);
    [DllImport("winspool.drv", CharSet=CharSet.Auto, SetLastError=true)]
    public static extern bool EndDocPrinter(IntPtr hPrinter);
    [DllImport("winspool.drv", CharSet=CharSet.Auto, SetLastError=true)]
    public static extern bool StartPagePrinter(IntPtr hPrinter);
    [DllImport("winspool.drv", CharSet=CharSet.Auto, SetLastError=true)]
    public static extern bool EndPagePrinter(IntPtr hPrinter);
    [DllImport("winspool.drv", CharSet=CharSet.Auto, SetLastError=true)]
    public static extern bool WritePrinter(IntPtr hPrinter, byte[] pBytes, int dwCount, out int dwWritten);
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Auto)]
    public struct DOCINFO {{
        [MarshalAs(UnmanagedType.LPTStr)] public string pDocName;
        [MarshalAs(UnmanagedType.LPTStr)] public string pOutputFile;
        [MarshalAs(UnmanagedType.LPTStr)] public string pDataType;
    }}
}}
'@ -ErrorAction Stop;
$hPrinter = [IntPtr]::Zero;
$opened = [RawPrinterHelper]::OpenPrinter('{name}', [ref]$hPrinter, [IntPtr]::Zero);
if (-not $opened) {{ throw "OpenPrinter failed for '{name}'" }};
$docInfo = New-Object RawPrinterHelper+DOCINFO;
$docInfo.pDocName  = 'PrintAgentJob';
$docInfo.pDataType = 'RAW';
$docInfo.pOutputFile = $null;
$docId = [RawPrinterHelper]::StartDocPrinter($hPrinter, 1, [ref]$docInfo);
if ($docId -le 0) {{ [RawPrinterHelper]::ClosePrinter($hPrinter); throw "StartDocPrinter failed" }};
[RawPrinterHelper]::StartPagePrinter($hPrinter) | Out-Null;
$written = 0;
$ok = [RawPrinterHelper]::WritePrinter($hPrinter, $bytes, $bytes.Length, [ref]$written);
[RawPrinterHelper]::EndPagePrinter($hPrinter) | Out-Null;
[RawPrinterHelper]::EndDocPrinter($hPrinter) | Out-Null;
[RawPrinterHelper]::ClosePrinter($hPrinter) | Out-Null;
if (-not $ok) {{ throw "WritePrinter failed - $written bytes written of $($bytes.Length)" }};
Write-Output "OK: $written bytes sent to '{name}'";
"#,
        temp = temp_file_path.replace('\\', "\\\\"),
        name = printer_name.replace('\'', "\\'")
    );

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|e| format!("Failed to spawn PowerShell: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    log::info!("WinSpool stdout: {}", stdout);
    if !stderr.is_empty() {
        log::warn!("WinSpool stderr: {}", stderr);
    }

    if !output.status.success() || stdout.is_empty() || !stdout.contains("OK:") {
        return Err(format!(
            "WinSpool raw print failed.\nstdout: {}\nstderr: {}",
            stdout, stderr
        ));
    }

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
            // Centered bold line at normal size — report titles/section
            // headings where a double-height "header" would be oversized.
            "subheader" => {
                set_alignment_center(&mut bytes);
                add_text_bold(&mut bytes, &line.text);
                bytes.push(LF);
                set_alignment_left(&mut bytes);
            }
            // Left-aligned bold line at normal size — report rows like
            // category names or totals (text is pre-padded by the sender).
            "bold" => {
                set_alignment_left(&mut bytes);
                add_text_bold(&mut bytes, &line.text);
                bytes.push(LF);
            }
            // Explicit plain left-aligned line — same output as the `_`
            // fallback, but lets senders be explicit about intent.
            "text" => {
                set_alignment_left(&mut bytes);
                add_text(&mut bytes, &line.text);
                bytes.push(LF);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn job(open_drawer: bool) -> PrintJob {
        serde_json::from_value(serde_json::json!({
            "jobId": "j1", "branchId": "b", "destinationZone": "z", "contentType": "receipt",
            "cutPaper": false, "priority": "normal", "createdAt": "",
            "openDrawer": open_drawer,
            "lines": [{ "type": "item", "text": "Tea", "qty": 2, "price": "100" }]
        }))
        .unwrap()
    }

    #[test]
    fn drawer_kick_follows_init() {
        let bytes = render_job(&job(true), PrinterLanguage::EscPos, DrawerPin::Pin2);
        assert_eq!(&bytes[..7], &[ESC, b'@', ESC, b'p', 0, 25, 250]);
        let plain = render_job(&job(false), PrinterLanguage::EscPos, DrawerPin::Pin2);
        assert!(!plain.windows(2).any(|w| w == [ESC, b'p']));
        assert_eq!(drawer_kick_bytes(DrawerPin::Both).len(), 10);
    }

    #[test]
    fn lines_fall_back_to_text_label() {
        let zpl = String::from_utf8(render_job(&job(true), PrinterLanguage::Zpl, DrawerPin::Pin2)).unwrap();
        assert!(zpl.contains("^FD2x Tea  100^FS"));
    }
}
