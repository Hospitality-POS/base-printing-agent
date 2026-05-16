# Biggie API — PrintAgent Implementation Guide

> Step-by-step instructions to add PrintAgent support to your existing `biggie_api` codebase.

---

## Overview

Three new files + two edits to `server.js`. Total time: ~30 minutes.

```
models/Agent/agent.js                    ← Mongoose schema
controllers/agentController/agent.js     ← POST /api/agents/pair handler
services/agentWebSocket.js               ← WebSocket server + sendPrintJob()
server.js                                ← Mount endpoint + attach WebSocket
```

---

## Step 1: Install `ws` package

```bash
cd /path/to/biggie_api
yarn add ws
```

---

## Step 2: Create Agent Model

**File:** `models/Agent/agent.js`

```js
const mongoose = require("mongoose");

const AgentSchema = new mongoose.Schema({
  agentId: { type: String, required: true, unique: true },
  deviceToken: { type: String, required: true }, // bcrypt-hashed
  shop_id: { type: String, required: true },    // maps to branchId on the wire
  zone: { type: String, required: true },      // "kitchen" | "bar" | "cashier"
  deviceName: { type: String, required: true },
  companyId: { type: String, required: true },
  main_category_id: { type: mongoose.Schema.Types.ObjectId }, // assigned from frontend
  isActive: { type: Boolean, default: true },
  createdAt: { type: Date, default: Date.now },
});

module.exports = mongoose.model("Agent", AgentSchema);
```

---

## Step 3: Create Pairing Controller

**File:** `controllers/agentController/agent.js`

```js
const Agent = require("../../models/Agent/agent");
const bcrypt = require("bcrypt");

exports.pair = async (req, res) => {
  try {
    const { companyCode, branchId, zone, deviceName } = req.body;

    // Validate required fields
    if (!companyCode || !branchId || !zone || !deviceName) {
      return res.status(400).json({ error: "Missing required fields" });
    }

    // Validate companyCode against your company lookup
    // This is a placeholder — replace with your actual company validation
    const companyId = await validateCompanyCode(companyCode);
    if (!companyId) {
      return res.status(404).json({ error: "Company code not found" });
    }

    // Check if an agent already exists for this shop + zone
    const existing = await Agent.findOne({ shop_id: branchId, zone, isActive: true });
    if (existing) {
      return res.status(409).json({ error: "A printer agent is already registered for this zone" });
    }

    // Generate credentials
    const agentId = `agt_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    const deviceToken = `tok_${require("crypto").randomBytes(16).toString("hex")}`;
    const hashedToken = await bcrypt.hash(deviceToken, 10);

    // Save to DB
    await Agent.create({
      agentId,
      deviceToken: hashedToken,
      shop_id: branchId,
      zone,
      deviceName,
      companyId,
    });

    // Return credentials to the agent (unhashed token!)
    const protocol = req.protocol === "https" ? "wss" : "ws";
    const host = req.get("host");
    const wsUrl = `${protocol}://${host}/ws/agent`;

    res.json({
      agentId,
      deviceToken, // unhashed — agent stores this
      wsUrl,
      companyId,
    });
  } catch (err) {
    console.error("[pair] Error:", err);
    res.status(500).json({ error: "Internal server error" });
  }
};

// Helper: validate company code against your existing Company model
async function validateCompanyCode(code) {
  // Use your existing Company model or dbConnectionManager
  const Company = require("../../models/Company/company"); // adjust path as needed
  const company = await Company.findOne({ companyCode: code });
  return company ? company._id.toString() : null;
}
```

---

## Step 4: Create WebSocket Service

**File:** `services/agentWebSocket.js`

```js
const { WebSocketServer } = require("ws");
const Agent = require("../models/Agent/agent");
const bcrypt = require("bcrypt");

let wss = null;
const agents = new Map(); // agentId → WebSocket

exports.attach = (httpServer) => {
  if (wss) {
    console.warn("[ws] WebSocket server already attached");
    return;
  }

  wss = new WebSocketServer({ noServer: true });

  // Handle upgrade requests for /ws/agent
  httpServer.on("upgrade", (req, socket, head) => {
    if (req.url === "/ws/agent") {
      wss.handleUpgrade(req, socket, head, (ws) => {
        wss.emit("connection", ws, req);
      });
    } else {
      socket.destroy();
    }
  });

  wss.on("connection", (ws, req) => {
    let agentMeta = null;
    let authTimer = null;

    console.log("[ws] Connection from", req.socket.remoteAddress);

    // 5-second auth timeout
    authTimer = setTimeout(() => {
      if (!agentMeta) {
        console.warn("[ws] Auth timeout — closing connection");

        // Validate auth message
        ws.on("message", async (data) => {
          try {
            const msg = JSON.parse(data.toString());

            if (msg.type === "auth") {
              console.log("[ws] Auth attempt:", msg);

              if (!msg.deviceToken) {
                console.error("[ws] Auth failed: Missing deviceToken");
                ws.send(JSON.stringify({ type: "auth_error", message: "deviceToken required" }));
                ws.close(1008, "Missing deviceToken");
                return;
              }

              if (!msg.agentId) {
                console.error("[ws] Auth failed: Missing agentId");
                ws.send(JSON.stringify({ type: "auth_error", message: "agentId required" }));
                ws.close(1008, "Missing agentId");
                return;
              }

              if (!msg.companyCode) {
                console.error("[ws] Auth failed: Missing companyCode");
                ws.send(JSON.stringify({ type: "auth_error", message: "companyCode required" }));
                ws.close(1008, "Missing companyCode");
                return;
              }

              // Find agent in database
              const agent = await Agent.findOne({ agentId: msg.agentId });
              if (!agent) {
                console.error("[ws] Auth failed: Agent not found:", msg.agentId);
                ws.send(JSON.stringify({ type: "auth_error", message: "Agent not found" }));
                ws.close(1008, "Agent not found");
                return;
              }

              // Verify device token using bcrypt
              const valid = await bcrypt.compare(msg.deviceToken, agent.deviceToken);
              if (!valid) {
                console.error("[ws] Auth failed: Invalid token for agent:", msg.agentId);
                ws.send(JSON.stringify({ type: "auth_error", message: "Invalid token" }));
                ws.close(1008, "Invalid token");
                return;
              }

              // Authenticated successfully
              clearTimeout(authTimer);
              const agentMeta = {
                agentId: agent.agentId,
                shop_id: agent.shop_id,
                zone: agent.zone,
                deviceName: agent.deviceName,
                ws,
              };
              agents.set(agent.agentId, ws);
              console.log("[ws] Authenticated:", agent.agentId, "zone:", agent.zone);

              ws.send(JSON.stringify({ type: "auth_ok", message: "Authenticated successfully" }));
              return;
            }
          } catch (err) {
            console.error("[ws] Message error:", err);
          }
        });
      }
    }, 5000);

    ws.on("message", async (data) => {
      try {
        const msg = JSON.parse(data.toString());

        }

        if (msg.type === "print_ack") {
          console.log(`[ws] ACK: job=${msg.jobId} status=${msg.status}`);
          // TODO: Update job status in DB if you track print jobs
          return;
        }

        if (msg.type === "pong") return;
      } catch (err) {
        console.error("[ws] Message error:", err);
      }
    });

    ws.on("close", () => {
      if (agentMeta) {
        agents.delete(agentMeta.agentId);
        console.log("[ws] Disconnected:", agentMeta.agentId);
      }
      if (authTimer) clearTimeout(authTimer);
    });

    ws.on("error", (err) => {
      console.error("[ws] WebSocket error:", err);
    });
  });

  console.log("[ws] WebSocket server attached to /ws/agent");
};

// Send a print job to all agents for a shop, or a specific agent
exports.sendPrintJob = (shop_id, agentId, job) => {
  let sent = 0;
  for (const [aid, ws] of agents) {
    if (ws.readyState !== ws.OPEN) continue;

    // If agentId is specified, route only to that agent
    if (agentId && aid !== agentId) continue;

    // Otherwise, broadcast to all agents for this shop
    const meta = getAgentMeta(aid);
    if (!meta || meta.shop_id !== shop_id) continue;

    // Optional: zone filtering if destinationZone is set
    if (job.destinationZone && meta.zone !== job.destinationZone) continue;

    ws.send(JSON.stringify({ type: "print_job", job }));
    sent++;
  }
  console.log(`[ws] Sent print job ${job.jobId} to ${sent} agent(s)`);
  return sent;
};

// Helper: get stored agent metadata
function getAgentMeta(agentId) {
  // In production, you might cache this or query the DB
  // For now, we store minimal metadata in the Map
  return agents.get(agentId)?.agentMeta;
}
```

---

## Step 5: Update `server.js`

Add these two edits to your `server.js`:

### Edit 1: Mount the pairing endpoint (BEFORE `tenantDBMiddleware`)

Find the line where you mount `/webhook` or `/printer-service/render-receipt/:jobId` and add this **before** the `tenantDBMiddleware` block:

```js
// Inside runServer(), BEFORE tenantDBMiddleware
const agentController = require("./controllers/agentController/agent");
app.post("/api/agents/pair", agentController.pair);
```

### Edit 2: Attach the WebSocket server (AFTER `app.listen()`)

Find where you call `app.listen()` and add the WebSocket attachment:

```js
const agentWs = require("./services/agentWebSocket");

// ... existing code ...

const httpServer = app.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

// Attach WebSocket to the same server
agentWs.attach(httpServer);
```

---

## Step 6: Test Locally

1. Restart your `biggie_api` server
2. Open PrintAgent app
3. Enter:
   - Company Code: `TEST` (or your actual code)
   - Branch ID: any identifier (e.g., `shop-001`)
   - Zone: `kitchen`
   - Device Name: `Test Printer`
4. Click **Pair Device**
5. Should see success and agent connects to WebSocket

---

## Step 7: Send a Test Print Job

Add this test route temporarily to `server.js`:

```js
app.get("/api/test-print", async (req, res) => {
  const { sendPrintJob } = require("./services/agentWebSocket");

  const job = {
    jobId: `job_${Date.now()}_test`,
    branchId: "shop-001",
    destinationZone: "kitchen",
    contentType: "test",
    cutPaper: true,
    priority: "normal",
    createdAt: new Date().toISOString(),
    lines: [
      { type: "header", text: "TEST PRINT" },
      { type: "divider" },
      { type: "item", text: "Test Item", qty: 1, price: "KES 100" },
      { type: "footer", text: "If you see this, it works!" },
    ],
  };

  sendPrintJob("shop-001", null, job);
  res.json({ message: "Print job sent" });
});
```

Visit `http://localhost:3002/api/test-print` — your connected printer should print the test ticket.

---

## Step 8: Integrate with Order Creation

In `controllers/ordersController/orders.js`, after saving an order:

```js
const { sendPrintJob } = require("../../services/agentWebSocket");

// Inside your order creation handler, after saving:
const job = {
  jobId: `job_${Date.now()}_${order._id}`,
  branchId: order.shop_id,
  destinationZone: null,
  contentType: "food_order",
  cutPaper: true,
  priority: "normal",
  createdAt: new Date().toISOString(),
  lines: [
    { type: "header", text: `ORDER #${order.order_no}` },
    ...cart_items.map((item) => ({
      type: "item",
      text: item.product_id?.name || item.product_id?.product_name || "Item",
      qty: item.quantity,
      price: `KES ${(item.price * item.quantity).toFixed(2)}`,
    })),
    { type: "footer", text: order.table_id ? `Table ${order.table_id}` : "Takeaway" },
  ],
};

sendPrintJob(order.shop_id, null, job);
```

---

## Field Mapping Cheat Sheet

| PrintAgent sends | You store as | Notes |
|------------------|--------------|-------|
| `branchId` | `shop_id` | Map on receive, map back on send |
| `companyCode` | `companyCode` | No change |
| `deviceToken` | `deviceToken` (hashed) | Hash before storing, compare on auth |

---

## Security Checklist

- [ ] `deviceToken` is bcrypt-hashed in DB (never store plain)
- [ ] Pairing endpoint is **before** `tenantDBMiddleware` (no auth header needed)
- [ ] WebSocket auth timeout closes connection after 5 s
- [ ] Use `wss://` in production (TLS required)
- [ ] Validate `shop_id` matches the shop in jobs you send

---

## Troubleshooting

| Issue | Fix |
|-------|-----|
| 405 on `/api/agents/pair` | Check that route is mounted in `server.js` before `tenantDBMiddleware` |
| WebSocket won't connect | Verify `agentWs.attach(httpServer)` is called **after** `app.listen()` |
| Auth always fails | Check bcrypt compare — ensure you're comparing with the hashed token |
| Print job not received | Check `sendPrintJob` is called with correct `shop_id` |

---

## Next Steps

After this works:
1. Add `POST /api/agents/print` endpoint for frontend reprint requests
2. Add `GET /api/agents/connected?shop_id=...` for printer status UI
3. Implement token rotation endpoint for compromised devices

See `POS_API_INTEGRATION.md` for full frontend integration guide.
