# PrintAgent

Silent, always-on print routing agent for thermal printers. Connects a branch's local USB thermal printer to a multi-tenant SaaS backend via authenticated WebSocket.

## Tech Stack

- **Tauri v2** — Rust backend + React/TypeScript frontend
- **Rust** — WebSocket client, ESC/POS printing, SQLite queue
- **React + TypeScript + Tailwind CSS** — Setup & status UI
- **SQLite** via `sqlx` — Local job queue + deduplication
- **tokio** — Async Rust runtime
- **tokio-tungstenite** — WebSocket client with reconnect loop
- **serialport** — COM/USB port access for thermal printers

## Prerequisites

- [Rust](https://rustup.rs/) (stable, 1.75+)
- [Node.js](https://nodejs.org/) 18+
- Tauri v2 system dependencies → see https://tauri.app/start/prerequisites/
- On Windows: Visual C++ Build Tools

## Quick Start

### 1. Install dependencies

```bash
npm install
```

### 2. Generate app icons

Provide a 1024×1024 PNG source image and run:

```bash
chmod +x scripts/generate-icons.sh
./scripts/generate-icons.sh path/to/your-logo-1024.png
```

This runs `npx @tauri-apps/cli icon` which generates all required icon sizes in `src-tauri/icons/`.

### 3. Configure environment (optional)

```bash
cp .env.example .env
# Edit .env with your backend API URL
```

### 4. Run in development mode

```bash
# Start the mock backend server (in a separate terminal)
npm install ws           # only once
node scripts/mock-ws-server.js

# Start Tauri dev
npm run tauri dev
```

Set `PRINT_AGENT_API_URL=http://localhost:3131` when using the mock server.

### 5. Build for production

```bash
npm run tauri build
```

Produces installers in `src-tauri/target/release/bundle/`.

---

## Project Structure

```
print-agent/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs        # Entry point
│   │   ├── lib.rs         # Tauri app setup, tray, AppState
│   │   ├── config.rs      # Load/save config.json
│   │   ├── ws_client.rs   # WebSocket loop: connect, auth, reconnect
│   │   ├── printer.rs     # ESC/POS byte writer (serialport / /dev/usb/lp*)
│   │   ├── job_queue.rs   # SQLite queue: insert, mark_printed, dedup
│   │   ├── job_handler.rs # Receives job, dedup check, print, ack
│   │   └── commands.rs    # Tauri commands exposed to frontend
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/
│   ├── App.tsx                  # Root: SetupScreen or StatusScreen
│   ├── screens/
│   │   ├── SetupScreen.tsx      # Pairing form
│   │   └── StatusScreen.tsx     # Live status dashboard
│   └── hooks/
│       └── useAgentStatus.ts    # Polls Tauri every 2s for agent status
├── scripts/
│   ├── mock-ws-server.js        # Mock backend for local dev/testing
│   └── generate-icons.sh        # Icon generation helper
└── README.md
```

---

## Data Flow

```
Backend WS ──► ws_client.rs ──► job_handler.rs ──► printer.rs ──► USB Thermal Printer
                                      │
                                 job_queue.rs (SQLite)
                                      │
                                 Dedup by jobId
                                      │
                                 ack ──► Backend WS
```

**Offline mode:** Jobs are stored in SQLite as `pending`. On reconnect, `drain_pending_queue()` is called automatically.

---

## Configuration (config.json)

Stored at the Tauri `app_data_dir` (e.g. `%APPDATA%/com.printAgent.app/` on Windows):

```json
{
  "agentId": "uuid-v4",
  "companyId": "string",
  "branchId": "string",
  "zone": "kitchen",
  "deviceName": "Kitchen Printer",
  "deviceToken": "tok_...",
  "printerPort": "COM3",
  "paperWidth": 80,
  "wsUrl": "wss://your-saas.com/ws/agent"
}
```

---

## Tauri Commands (Frontend API)

| Command | Description |
|---------|-------------|
| `get_available_ports()` | Lists COM/USB ports |
| `get_config()` | Returns current config or `null` |
| `get_agent_status()` | Returns WS status, zone, queue count, last job |
| `pair_device(...)` | Calls backend `/api/agents/pair`, saves config, starts WS loop |
| `test_print()` | Sends test ESC/POS bytes to configured printer |
| `unpair_device()` | Removes config, disconnects |

---

## Mock Server

The mock server in `scripts/mock-ws-server.js` provides:
- `POST http://localhost:3131/api/agents/pair` — Returns a fake `agentId`, `deviceToken`, `wsUrl`
- `ws://localhost:8765` — Accepts auth, sends a test print job to the agent's zone every 15 seconds

```bash
node scripts/mock-ws-server.js
```

---

## ESC/POS Line Types

| `type` | Rendered As |
|--------|-------------|
| `header` | Bold + double-height, centered |
| `item` | `qty x text ......... price`, left-aligned |
| `divider` | `------------------------------------------` |
| `total` | Bold + double-height, right-aligned |
| `footer` | Normal text, centered |

---

## Windows Printer Ports

On Windows, list available ports with Device Manager or via the app's port dropdown. Common ports: `COM3`, `COM4`, etc. The app uses the `serialport` crate (9600 baud by default for ESC/POS).

## Linux Printer Ports

USB thermal printers on Linux typically appear as `/dev/usb/lp0` or `/dev/lp0`. The `printer.rs` module detects these and uses direct file-write instead of serial protocol.
