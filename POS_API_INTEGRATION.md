# POS API Integration Guide — PrintAgent

This document describes every endpoint and WebSocket contract your cloud POS backend must implement for PrintAgent to pair, connect, and receive print jobs.

---

## Overview

```
PrintAgent (Windows / macOS desktop)
        │
        │  1. POST /api/agents/pair   (once, on first setup)
        │  2. WS  wss://…/ws/agent    (persistent, auto-reconnect)
        ▼
  Your Cloud POS API
        │
        │  push { type: "print_job" } whenever an order is placed
        ▼
  Thermal Printer (USB serial or TCP/IP network)
```

The agent:
- Runs silently in the Windows/macOS system tray
- Reconnects automatically every 3 s if the WebSocket drops
- Queues jobs locally in SQLite and drains the queue on reconnect
- Filters jobs by **zone** — a single backend can serve multiple printers (kitchen, bar, cashier, etc.)

---

## Environment

| Variable | Description |
|----------|-------------|
| `PRINT_AGENT_API_URL` | Base URL of your API, no trailing slash. E.g. `https://api.yourpos.com` |

Set via `.env` file placed next to the executable, or as a system environment variable.

---

## 1. Pairing Endpoint

### `POST {PRINT_AGENT_API_URL}/api/agents/pair`

Called **once** when the cashier configures a new device. Authenticates the device and returns credentials the agent stores permanently.

### Request

```json
{
  "companyCode": "ACME",
  "branchId":    "nairobi-001",
  "zone":        "kitchen",
  "deviceName":  "Kitchen Printer 1"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `companyCode` | `string` | Company or organisation code entered by the user |
| `branchId` | `string` | Branch identifier |
| `zone` | `string` | Print zone this device handles (`kitchen`, `bar`, `cashier`, etc.) |
| `deviceName` | `string` | Human-readable label for this device |

### Response `200 OK`

```json
{
  "agentId":     "agt_abc123",
  "deviceToken": "tok_xxxxxxxxxxxxxxxx",
  "wsUrl":       "wss://api.yourpos.com/ws/agent",
  "companyId":   "company_456"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `agentId` | `string` | Unique identifier for this agent instance |
| `deviceToken` | `string` | Secret token used to authenticate the WebSocket connection |
| `wsUrl` | `wss://` URL | WebSocket endpoint the agent will connect to |
| `companyId` | `string` | Your internal company ID (stored in config, not used for auth) |

### Error responses

```json
{ "error": "Company code not found" }        // 404
{ "error": "Branch not authorised" }         // 403
{ "error": "Zone already has an active agent" } // 409 (optional)
```

The agent surfaces the error message directly in the setup screen.

---

## 2. WebSocket Connection

### `GET {wsUrl}` — Upgrade to WebSocket

The agent connects using **`tokio-tungstenite`** (standard WebSocket, RFC 6455).

**Headers sent by the agent:**
```
Connection: Upgrade
Upgrade: websocket
```

No custom headers are used for auth — authentication happens via the first message after the handshake (see below).

---

## 3. WebSocket Message Protocol

All messages are **JSON text frames**.

### 3.1 Auth (agent → server) — first message after connect

Sent immediately after the WebSocket handshake is complete.

```json
{
  "type":        "auth",
  "deviceToken": "tok_xxxxxxxxxxxxxxxx",
  "agentId":     "agt_abc123",
  "zone":        "kitchen",
  "branchId":    "nairobi-001"
}
```

### 3.2 Auth OK (server → agent) — required reply

The server **must** respond with this or the agent will close and retry.

```json
{
  "type":    "auth_ok",
  "message": "Authenticated successfully"
}
```

### 3.3 Print Job (server → agent)

Send this whenever an order is placed that should be printed.

```json
{
  "type": "print_job",
  "job": {
    "jobId":           "job_1716900000_001",
    "branchId":        "nairobi-001",
    "destinationZone": "kitchen",
    "contentType":     "food_order",
    "cutPaper":        true,
    "priority":        "normal",
    "createdAt":       "2026-05-16T17:00:00Z",
    "lines": [
      { "type": "header",  "text": "ORDER #1042" },
      { "type": "divider", "text": "" },
      { "type": "item",    "text": "Chicken Wings",  "qty": 2, "price": "KES 800" },
      { "type": "item",    "text": "Fries",          "qty": 1, "price": "KES 200" },
      { "type": "divider", "text": "" },
      { "type": "total",   "text": "TOTAL: KES 1,000" },
      { "type": "footer",  "text": "Table 7 — Dine In" }
    ]
  }
}
```

#### Job fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `jobId` | `string` | ✅ | Globally unique job ID. Used for deduplication — the agent will **not** print the same `jobId` twice |
| `branchId` | `string` | ✅ | Must match the agent's configured `branchId` |
| `destinationZone` | `string` | ✅ | The agent **silently ignores** jobs where this doesn't match its own `zone` |
| `contentType` | `string` | ✅ | Informational. E.g. `food_order`, `receipt`, `label` |
| `cutPaper` | `boolean` | ✅ | Whether to send an ESC/POS paper-cut command after printing |
| `priority` | `string` | ✅ | `normal` or `urgent` (currently informational, not yet acted on) |
| `createdAt` | ISO 8601 | ✅ | Job creation timestamp |
| `lines` | `Line[]` | ✅ | Array of print lines (see below) |

#### Line types

| `type` | Extra fields | Printed as |
|--------|-------------|------------|
| `header` | `text` | **Bold, double-height, centred** |
| `item` | `text`, `qty?`, `price?` | Two-column: `{qty}x {text}` … `{price}` |
| `divider` | — | `------------------------------------------` |
| `total` | `text` | Bold, double-height, right-aligned |
| `footer` | `text` | Normal, centred |
| *(any other)* | `text` | Plain left-aligned text |

### 3.4 Print ACK (agent → server) — after every job attempt

```json
{
  "type":    "print_ack",
  "jobId":   "job_1716900000_001",
  "status":  "printed",
  "agentId": "agt_abc123"
}
```

| `status` | Meaning |
|----------|---------|
| `printed` | ESC/POS bytes written to printer successfully |
| `failed` | Printer unreachable or write error (job is saved in local SQLite queue and retried on next reconnect) |

### 3.5 Ping / Pong (keepalive)

**Server → Agent:**
```json
{ "type": "ping" }
```

**Agent → Server (automatic reply):**
```json
{ "type": "pong" }
```

The server should send a ping every 30 s. The agent also handles WebSocket-level `PING` frames automatically.

---

## 4. Zone Routing

The agent only prints jobs where `destinationZone === agent.zone`. Your backend is responsible for routing:

```
Order placed
    │
    ├─ Has kitchen items?  → send print_job with destinationZone: "kitchen"
    ├─ Has bar items?      → send print_job with destinationZone: "bar"
    └─ Always?             → send print_job with destinationZone: "cashier"
```

You can broadcast the same WebSocket message to **all connected agents** for a branch — each agent self-filters. Or maintain a per-zone connection map and send only to the right agent.

---

## 5. Deduplication

The agent stores every printed `jobId` in a local SQLite database. If the same `jobId` arrives again (e.g. after a reconnect), it is silently dropped. **Always use a unique `jobId` per print event.** Recommended format:

```
job_{timestamp_ms}_{incrementing_counter}
```

---

## 6. Reconnect Behaviour

| Scenario | Agent behaviour |
|----------|----------------|
| Network drop | Reconnects every 3 s, re-sends `auth` on each new connection |
| Server restart | Same as network drop |
| Auth failure (no `auth_ok`) | Closes and retries after 3 s |
| Job arrives while disconnected | Queued in SQLite, drained automatically after next successful auth |

---

## 7. Example Server Implementation (Node.js / Express + ws)

```js
import { WebSocketServer } from "ws";
import express from "express";

const app = express();
app.use(express.json());

// Map of agentId → WebSocket for broadcasting
const agents = new Map();

// Pairing endpoint
app.post("/api/agents/pair", (req, res) => {
  const { companyCode, branchId, zone, deviceName } = req.body;

  // TODO: validate companyCode against your DB
  const agent = {
    agentId:     `agt_${crypto.randomUUID()}`,
    deviceToken: `tok_${crypto.randomBytes(16).toString("hex")}`,
    wsUrl:       "wss://api.yourpos.com/ws/agent",
    companyId:   "company_001",
  };

  // TODO: save agent to DB
  res.json(agent);
});

// WebSocket server
const wss = new WebSocketServer({ port: 8765 });

wss.on("connection", (ws) => {
  let agentMeta = null;

  ws.on("message", (data) => {
    const msg = JSON.parse(data.toString());

    if (msg.type === "auth") {
      // TODO: validate msg.deviceToken against DB
      agentMeta = { agentId: msg.agentId, zone: msg.zone, branchId: msg.branchId };
      agents.set(msg.agentId, ws);
      ws.send(JSON.stringify({ type: "auth_ok", message: "Authenticated successfully" }));
      return;
    }

    if (msg.type === "print_ack") {
      // TODO: update job status in DB
      console.log(`ACK: job=${msg.jobId} status=${msg.status}`);
      return;
    }

    if (msg.type === "pong") return;
  });

  ws.on("close", () => {
    if (agentMeta) agents.delete(agentMeta.agentId);
  });
});

// Send a print job to a specific zone in a branch
export function sendPrintJob(branchId, zone, job) {
  for (const [, ws] of agents) {
    // The agent self-filters by zone, so broadcasting is safe
    if (ws.readyState === ws.OPEN) {
      ws.send(JSON.stringify({ type: "print_job", job }));
    }
  }
}

app.listen(3131);
```

---

## 8. Security Recommendations

| Concern | Recommendation |
|---------|---------------|
| `deviceToken` exposure | Store hashed (bcrypt) in DB; treat like a password |
| WebSocket auth timeout | Close unauthenticated connections after 5 s |
| HTTPS / WSS | Always use TLS in production (`wss://`) |
| Token rotation | Provide a `POST /api/agents/rotate-token` endpoint for compromised devices |
| Zone isolation | Validate that the agent's `branchId` (= `shop_id` internally) matches the `destinationZone` in jobs you send it |

---

## 9. Codebase-Specific Implementation Notes

> These notes map the spec above onto the existing `biggie_api` project structure.

### 9.0 Field name mapping (spec → codebase)

| Spec field | Internal codebase field | Notes |
|------------|------------------------|-------|
| `branchId` | `shop_id` | All spec messages use `branchId` on the wire; store and query as `shop_id` internally |
| `companyCode` | `companyCode` | Same name — no translation needed |

> **Rule:** When the agent sends `branchId` (in pairing or WebSocket auth), read it as `shop_id` in your controllers/models. When building a `print_job` to send, write your internal `shop_id` into the `branchId` field.

### 9.1 Pairing endpoint — where to mount it

Mount `POST /api/agents/pair` **before** `tenantDBMiddleware` in `server.js`, exactly like `/webhook` and `/printer-service/render-receipt/:jobId`:

```js
// server.js — inside runServer(), BEFORE the tenantDBMiddleware block
const agentController = require("./controllers/agentController/agent");
app.post("/api/agents/pair", agentController.pair);
```

**Why:** The pairing request comes from a fresh device that has not yet authenticated. It carries `companyCode` in the request body (not the `companycode` header that `tenantDBMiddleware` expects), so it must bypass that middleware.

**`companyCode` resolution:** Look up the company in the shared/master DB (`dbConnectionManager`), not a tenant DB. Return `companyId` once found.

**`branchId` mapping:** The request body contains `branchId`; map it to `shop_id` when saving the Agent document and when filtering WebSocket connections.

### 9.2 WebSocket server — how to attach it

Attach the WebSocket server to the **existing `http.Server`** instance on path `/ws/agent`:

```js
// server.js — after app.listen() returns the server handle
const agentWs = require("./services/agentWebSocket");
const httpServer = app.listen(PORT, () => { ... });
agentWs.attach(httpServer);
```

The agent connects to `wss://api.hospitality.reliatech.co.ke/ws/agent` on the same port as the REST API — no firewall changes needed.

### 9.3 Authentication — no changes to existing middleware

`verifyToken` and `tenantDBMiddleware` in `middleware/Auth/authMiddleware.js` are **untouched**.

PrintAgent auth is self-contained: the `deviceToken` (bcrypt-hashed) in the `Agent` model is validated inside the WebSocket `auth` message handler in `services/agentWebSocket.js`.

### 9.4 New files to create

| File | Purpose |
|------|---------|
| `models/Agent/agent.js` | Mongoose schema — `agentId`, `deviceToken` (hashed), `shop_id`, `zone`, `deviceName`, `companyId`, `isActive` |
| `controllers/agentController/agent.js` | `pair()` handler — validates `companyCode`, creates Agent doc, returns credentials |
| `routers/agentRoute/agent.js` | Express router — `POST /pair`; token-rotation endpoint goes here later |
| `services/agentWebSocket.js` | `attach(httpServer)` + message handlers + `sendPrintJob(shop_id, zone, job)` export |

### 9.5 Existing printer files — unchanged

| File | Role |
|------|------|
| `routers/printerServiceRoute.js` | IP/TCP direct-print (store-receipt, render-receipt, etc.) |
| `routers/printerRoute/printer.js` | Printer config CRUD |
| `controllers/printerController.js` | Receipt rendering, test print, discover |
| `print-relay-agent/` | PrintAgent binary/installer will live here |

### 9.6 Zone routing hook

After an order is saved in `controllers/ordersController/orders.js`, call:

```js
const { sendPrintJob } = require("../../services/agentWebSocket");

// inside the order-creation handler, after saving:
sendPrintJob(order.shop_id, "kitchen", kitchenJob);
sendPrintJob(order.shop_id, "cashier", receiptJob);
```

When building the job object, set `job.branchId = order.shop_id` so the wire format matches what the agent expects.

### 9.7 `jobId` format

```js
const jobId = `job_${Date.now()}_${order._id}`;
```

`order._id` (MongoDB ObjectId) is globally unique and avoids a counter.

### 9.8 `ws` package

Check `package.json`. If not present:

```bash
yarn add ws
```

No other new runtime dependencies are required.

---

## 10. PrintAgent — Authentication & Environment Guide

> **Audience:** developer building or configuring the PrintAgent desktop application (Windows / macOS).

### 10.1 Environment — Local vs Production

Place a `.env` file next to the agent executable **or** set a system environment variable.

| Environment | `PRINT_AGENT_API_URL` |
|-------------|----------------------|
| **Production** | `https://api.hospitality.reliatech.co.ke` |
| **Local dev** | `http://localhost:3002` |

The `wsUrl` is returned by the server in the pairing response and stored in local config — the agent uses it as-is. The API URL is only needed for the initial pairing call.

### 10.2 First-Time Setup — Pairing Flow

```
Agent setup screen
        │
        │  User enters: Company Code, Branch ID, Zone, Device Name
        ▼
POST {PRINT_AGENT_API_URL}/api/agents/pair

Request body:
{
  "companyCode": "<entered by user>",
  "branchId":    "<entered by user>",   ← stored as shop_id on the server
  "zone":        "kitchen",
  "deviceName":  "Kitchen Printer 1"
}
        │
        │  200 OK
        ▼
Saved to local config.json:
{
  "agentId":     "agt_xxxx",
  "deviceToken": "tok_xxxx",   ← treat like a password — never log it
  "wsUrl":       "wss://…/ws/agent",
  "companyId":   "…",
  "zone":        "kitchen",
  "branchId":    "<entered by user>"
}
```

#### Pairing error handling

| HTTP status | Displayed to user |
|-------------|------------------|
| `400` | "Missing required fields — check all inputs" |
| `404` | "Company code not found — check the code and try again" |
| `403` | "Branch not authorised — contact your administrator" |
| `409` | "A printer agent is already registered for this zone — unpair it first" |
| `5xx` | "Server unreachable — check your internet connection" |

### 10.3 Credential Storage

| OS | Path |
|----|------|
| Windows | `%APPDATA%\PrintAgent\config.json` |
| macOS | `~/Library/Application Support/PrintAgent/config.json` |

Never store `deviceToken` in plain environment variables or the registry.

### 10.4 WebSocket Authentication Sequence

Every time the agent (re-)connects:

```
Agent                                    Server
  │── TCP/TLS handshake ────────────────▶│
  │◀─ 101 Switching Protocols ───────────│
  │                                       │
  │── { type: "auth",          ──────────▶│  sent immediately after handshake
  │     deviceToken: "tok_…",             │
  │     agentId:     "agt_…",             │
  │     zone:        "kitchen",           │
  │     branchId:    "nairobi-001" }      │
  │                                       │
  │◀─ { type: "auth_ok" } ───────────────│  agent is now active
       OR
  │◀─ { type: "auth_error",              │  token invalid / agent unpaired
  │     message: "…" }                   │
```

**On `auth_error`:** do not retry with the same credentials. Show notification: *"Printer agent authentication failed — please re-pair the device."*

**On timeout (no `auth_ok` within 5 s):** treat as network error, retry after 3 s.

### 10.5 Reconnect Loop

```
on_disconnect():
    wait 3 seconds
    open new WebSocket to wsUrl
    send auth message immediately
    if auth_ok  → resume normal operation
    if auth_error → stop reconnecting, alert user
    if timeout  → repeat loop
```

The agent **must** re-send the full `auth` message on every new connection.

### 10.6 Handling Print Jobs

After `auth_ok`, the server pushes `print_job` messages at any time.

**Zone filtering (agent-side):**
```
if job.destinationZone != agent.zone → silently ignore, do not ACK
```

**Deduplication:**
```
if jobId already in local SQLite → silently ignore
else → print → record jobId → send print_ack
```

### 10.7 Keepalive

- Server sends `{ "type": "ping" }` every **30 s** — agent replies with `{ "type": "pong" }`
- WebSocket-level `PING` frames are handled automatically by `tokio-tungstenite`
- If no message of any kind is received for **90 s** → assume dead connection → reconnect

### 10.8 Quick-Reference: What to Store

| Data | Store locally | Notes |
|------|--------------|-------|
| `agentId` | ✅ permanent | Used in every `auth` and `print_ack` |
| `deviceToken` | ✅ permanent | Used in every `auth` — treat like a password |
| `wsUrl` | ✅ permanent | Use as-is from pairing response |
| `zone` | ✅ permanent | Used for self-filtering jobs |
| `branchId` | ✅ permanent | Used in every `auth` |
| `companyId` | ✅ optional | Informational only |
| `jobId` (printed) | ✅ SQLite | Deduplication — keep indefinitely |
| `jobId` (failed) | ✅ SQLite queue | Retry on next reconnect |
| Print job `lines` | ❌ discard | No need to persist after printing |

---

## 11. Devices — Cash Drawer, Printer Status, Scales, Label Printers

PrintAgent also drives the other hardware attached to the till PC. Everything
below is **optional and backwards compatible**: servers that ignore these
messages keep working unchanged.

### 11.1 Capabilities in `auth`

The `auth` message now also carries what this agent can do:

```json
{
  "type": "auth",
  "agentId": "agt_abc123",
  "...": "existing fields",
  "capabilities": ["print", "status", "drawer", "scale"],
  "printerLanguage": "escpos",
  "cashDrawer": true,
  "scales": [{ "id": "scale_1a2b3c4d", "name": "Butchery scale", "protocol": "toledo" }],
  "localApiPort": 14210
}
```

`printerLanguage` is `escpos` (receipt), `zpl` or `tspl` (label printers).

### 11.2 New print job fields

| Field | Type | Description |
|-------|------|-------------|
| `openDrawer` | `boolean` | Kick the cash drawer wired to this receipt printer while the receipt prints. Ignored by label printers. |
| `label` | `LabelJob` | Layout used when the job is routed to a ZPL/TSPL printer. Without it, a label printer prints the `lines` as plain text rows. |

Both are also accepted on `/api/agents/pending-jobs` items (`open_drawer` / `openDrawer`, `label`).

#### `LabelJob`

All positions and sizes are millimetres from the top-left corner.

```json
{
  "widthMm": 50, "heightMm": 30, "gapMm": 2, "dpi": 203, "copies": 1,
  "elements": [
    { "type": "text",    "x": 2, "y": 2,  "text": "Beef mince", "sizeMm": 4 },
    { "type": "text",    "x": 2, "y": 8,  "text": "1.234 kg  KES 987" },
    { "type": "barcode", "x": 2, "y": 13, "data": "2012345012347", "symbology": "ean13", "heightMm": 10 },
    { "type": "qrcode",  "x": 36, "y": 2, "data": "https://example.com/o/1042", "cellSize": 4 },
    { "type": "line",    "x": 0, "y": 12, "width": 50, "height": 0.3 },
    { "type": "box",     "x": 0, "y": 0, "width": 50, "height": 30, "thicknessMm": 0.3 }
  ]
}
```

`symbology`: `code128` (default), `ean13`, `ean8`, `upca`, `code39`.
Set `"raw": "^XA...^XZ"` instead of `elements` to send pre-built ZPL/TSPL as-is.

### 11.3 Device messages (server → agent → server)

Every request may carry a `requestId`; the reply echoes it.

| Server sends | Agent replies |
|--------------|---------------|
| `{ "type": "open_drawer" }` | `{ "type": "drawer_ack", "status": "opened" \| "failed", "error"? }` |
| `{ "type": "printer_status_request" }` | `{ "type": "printer_status", "status": PrinterStatus, "checkedAt" }` |
| `{ "type": "scale_read", "scaleId"? }` | `{ "type": "scale_reading", "scaleId", "reading": WeightReading }` or `{ ..., "error" }` |
| `{ "type": "scale_command", "scaleId"?, "command": "tare" \| "zero" }` | `{ "type": "scale_command_ack", "status": "ok" \| "failed", "error"? }` |
| `{ "type": "print_label", "label": LabelJob }` | `{ "type": "label_ack", "status": "printed" \| "failed", "error"? }` |

Replies also include `agentId`. Omitting `scaleId` uses the first scale.

The agent also **pushes** `printer_status` on its own whenever the state
changes, and at least every 5 minutes (checked every 60 s):

```json
{
  "type": "printer_status",
  "agentId": "agt_abc123",
  "requestId": null,
  "checkedAt": "2026-10-18T20:15:05Z",
  "status": {
    "state": "warning",
    "reachable": true,
    "source": "escpos",
    "online": true,
    "coverOpen": false,
    "paperOut": false,
    "paperNearEnd": true,
    "error": false,
    "drawerPinHigh": true,
    "queuedJobs": null,
    "messages": ["Paper is running low"]
  }
}
```

`state` is `ok` · `warning` (paper low) · `error` (paper out, cover open, printer error, offline) ·
`offline` (port/IP unreachable) · `unknown` (reachable but the printer doesn't report status).
Status details come from ESC/POS `DLE EOT` (serial/network receipt printers) or the Windows
spooler (driver-installed printers). Label printers only report reachability.

#### `WeightReading`

```json
{ "weight": 1.234, "unit": "kg", "stable": true, "overload": false, "underZero": false, "raw": "01.234" }
```

`weight` is `null` while the scale only reports a status (e.g. still moving). Only
charge for a weight when `stable` is `true`.

### 11.4 Local device API (desktop app)

For anything a cashier waits on — reading the scale, popping the drawer —
the POS web app should call PrintAgent **directly on the same PC** instead of
going through the cloud. The desktop app listens on `http://127.0.0.1:14210`
(configurable in the app under *Device API*).

| Method & path | Body | Result |
|---------------|------|--------|
| `GET  /api/v1/health` | | `{ name, version }` |
| `GET  /api/v1/devices` | | printers (with latest status) and scales |
| `GET  /api/v1/printers/{agentId}/status[?refresh=1]` | | `{ agentId, status, checkedAt }` |
| `POST /api/v1/printers/{agentId}/print` | `PrintJob` | prints locally (no server ack) |
| `POST /api/v1/printers/{agentId}/label` | `LabelJob` | |
| `POST /api/v1/printers/{agentId}/drawer` | | |
| `POST /api/v1/drawer` | `{ "agentId"? }` | the printer marked "cash drawer", or the only receipt printer |
| `POST /api/v1/label` | `LabelJob` | first label printer |
| `GET  /api/v1/scales` | | scale configs |
| `GET  /api/v1/scales/{scaleId\|default}/weight` | | `WeightReading` + `scaleId` |
| `POST /api/v1/scales/{scaleId\|default}/tare` · `/zero` | | |

Every response is `{ "ok": true, "result": ... }` or `{ "ok": false, "error": "..." }`.

```js
const r = await fetch("http://127.0.0.1:14210/api/v1/scales/default/weight").then((r) => r.json());
if (r.ok && r.result.stable) addWeighedItem(r.result.weight, r.result.unit);

await fetch("http://127.0.0.1:14210/api/v1/drawer", { method: "POST" });
```

**Security.** The API binds to `127.0.0.1` only. Browser requests must come from
an allowed `Origin` (default: `https://*.reliatech.co.ke`, `http://localhost[:*]`,
`http://127.0.0.1[:*]`); others get `403`. Requests with no `Origin` header (native
apps, curl) are allowed. Preflights answer `Access-Control-Allow-Private-Network: true`,
which Chrome requires before an `https://` page may call a `127.0.0.1` address.

The legacy Windows 7 helper (`print-agent-helper`, port 14200) exposes the same
device operations through its `/cmd` endpoint (`open_cash_drawer`, `printer_status`,
`print_label`, `read_scale`, `scale_command`), taking the printer/scale settings in
the request instead of looking them up.

### 11.5 Supported scales

| Protocol | Typical models | Line settings |
|----------|----------------|---------------|
| `toledo` | Mettler Toledo 8217; Dibal, ACLAS, CAS in Toledo mode | 9600 7-E-1 |
| `nci` | Avery Weigh-Tronix, Brecknell, Salter | 9600 7-E-1 |
| `cas` | CAS continuous output (`ST,GS,+ 1.234kg`) | 9600 8-N-1 |
| `generic` | Any scale sending a line with the weight; optional request command | configurable |

Scales can be on a COM / USB-serial port or on the network (TCP).
