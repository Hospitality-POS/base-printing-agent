//! Shared low-level I/O: per-port locking and two-way (write + read back)
//! connections used by printer status queries and weighing scales.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Anything we can both write commands to and read replies from.
pub trait Duplex: Read + Write + Send {}
impl<T: Read + Write + Send + ?Sized> Duplex for T {}

fn lock_for(key: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    let map = LOCKS.get_or_init(Default::default);
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(key.trim().to_uppercase()).or_default().clone()
}

/// Run `f` while holding the exclusive lock for one physical device, so a
/// background status poll or scale read never collides with a print job on
/// the same COM port / socket.
pub fn with_port_lock<T>(key: &str, f: impl FnOnce() -> T) -> T {
    let lock = lock_for(key);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

/// Serial line settings. Most receipt printers use 9600 8N1; many scales use
/// 9600 7E1 (Toledo) or 8N1.
#[derive(Debug, Clone)]
pub struct SerialSettings {
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: String,
    pub stop_bits: u8,
}

impl Default for SerialSettings {
    fn default() -> Self {
        SerialSettings {
            baud_rate: 9600,
            data_bits: 8,
            parity: "none".to_string(),
            stop_bits: 1,
        }
    }
}

pub fn open_serial_duplex(port_name: &str, s: &SerialSettings) -> Result<Box<dyn Duplex>, String> {
    use serialport::{DataBits, Parity, StopBits};
    let data_bits = match s.data_bits {
        5 => DataBits::Five,
        6 => DataBits::Six,
        7 => DataBits::Seven,
        _ => DataBits::Eight,
    };
    let parity = match s.parity.to_lowercase().as_str() {
        "even" | "e" => Parity::Even,
        "odd" | "o" => Parity::Odd,
        _ => Parity::None,
    };
    let stop_bits = if s.stop_bits == 2 { StopBits::Two } else { StopBits::One };

    let port = serialport::new(port_name, s.baud_rate)
        .data_bits(data_bits)
        .parity(parity)
        .stop_bits(stop_bits)
        // Short per-read timeout; callers loop until their own deadline.
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| format!("Failed to open port {}: {}", port_name, e))?;
    let _ = port.clear(serialport::ClearBuffer::Input);
    Ok(Box::new(port))
}

pub fn open_tcp_duplex(host: &str, port: u16) -> Result<Box<dyn Duplex>, String> {
    use std::net::{TcpStream, ToSocketAddrs};
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Failed to resolve {}:{}: {}", host, port, e))?
        .next()
        .ok_or_else(|| format!("No address found for {}", host))?;
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|e| format!("Failed to connect to {}:{}: {}", host, port, e))?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_nodelay(true);
    Ok(Box::new(stream))
}

/// Read whatever arrives until `done(buffer)` is true or `total` elapses.
pub fn read_until(
    conn: &mut dyn Duplex,
    total: Duration,
    done: impl Fn(&[u8]) -> bool,
) -> Vec<u8> {
    let deadline = Instant::now() + total;
    let mut out = Vec::new();
    let mut chunk = [0u8; 256];
    while Instant::now() < deadline {
        match conn.read(&mut chunk) {
            // Socket closed by the peer.
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&chunk[..n]);
                if done(&out) {
                    break;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => break,
        }
    }
    out
}
