#!/usr/bin/env node
/**
 * Mock WebSocket + HTTP server for PrintAgent development.
 *
 * Implements:
 *  - POST /api/agents/pair  → returns mock agentId + deviceToken + wsUrl
 *  - WS  ws://localhost:8765  → accepts auth, sends a test print_job every 10s
 *
 * Usage:
 *   node scripts/mock-ws-server.js
 *
 * Requires: npm install ws
 */

import http from "http";
import { WebSocketServer } from "ws";

const HTTP_PORT = 3131;
const WS_PORT = 8765;

// ── HTTP pairing endpoint ───────────────────────────────────────────────────

const httpServer = http.createServer((req, res) => {
  if (req.method === "POST" && req.url === "/api/agents/pair") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      const data = JSON.parse(body);
      console.log("[PAIR] Received pairing request:", data);

      const response = {
        agentId: `agent-${Math.random().toString(36).slice(2, 10)}`,
        deviceToken: `tok_${Math.random().toString(36).slice(2, 18)}`,
        wsUrl: `ws://localhost:${WS_PORT}`,
        companyId: `company-${data.companyCode || "demo"}`,
      };

      console.log("[PAIR] Responding with:", response);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(response));
    });
  } else {
    res.writeHead(404);
    res.end("Not Found");
  }
});

httpServer.listen(HTTP_PORT, () => {
  console.log(`[HTTP] Pairing server running at http://localhost:${HTTP_PORT}`);
});

// ── WebSocket server ────────────────────────────────────────────────────────

const wss = new WebSocketServer({ port: WS_PORT });

wss.on("listening", () => {
  console.log(`[WS] WebSocket server running at ws://localhost:${WS_PORT}`);
});

wss.on("connection", (ws) => {
  console.log("[WS] Client connected.");
  let agentZone = "unknown";
  let agentId = "unknown";
  let jobCounter = 1;

  ws.on("message", (data) => {
    let msg;
    try {
      msg = JSON.parse(data.toString());
    } catch {
      console.warn("[WS] Invalid JSON received:", data.toString());
      return;
    }

    console.log("[WS] Received:", JSON.stringify(msg));

    switch (msg.type) {
      case "auth":
        agentZone = msg.zone || "default";
        agentId = msg.agentId || "unknown";
        console.log(
          `[WS] Agent authenticated: id=${agentId} zone=${agentZone}`
        );
        ws.send(
          JSON.stringify({ type: "auth_ok", message: "Authenticated successfully" })
        );

        setTimeout(() => sendTestJob(ws, agentZone, jobCounter++), 3000);
        break;

      case "print_ack":
        console.log(
          `[WS] Print ACK received: jobId=${msg.jobId} status=${msg.status}`
        );
        break;

      case "pong":
        console.log("[WS] Pong received.");
        break;

      default:
        console.log(`[WS] Unknown message type: ${msg.type}`);
    }
  });

  ws.on("close", () => console.log("[WS] Client disconnected."));
  ws.on("error", (err) => console.error("[WS] Error:", err));

  const pingInterval = setInterval(() => {
    if (ws.readyState === ws.OPEN) {
      ws.send(JSON.stringify({ type: "ping" }));
    }
  }, 30000);

  const jobInterval = setInterval(() => {
    if (ws.readyState === ws.OPEN) {
      sendTestJob(ws, agentZone, jobCounter++);
    }
  }, 15000);

  ws.on("close", () => {
    clearInterval(pingInterval);
    clearInterval(jobInterval);
  });
});

function sendTestJob(ws, zone, count) {
  const job = {
    jobId: `job-${Date.now()}-${count}`,
    branchId: "branch-nairobi-001",
    destinationZone: zone,
    contentType: "food_order",
    lines: [
      { type: "header", text: `ORDER #${1000 + count}` },
      { type: "divider", text: "" },
      { type: "item", text: "Chicken Wings", qty: 2, price: "KES 800" },
      { type: "item", text: "Fries", qty: 1, price: "KES 200" },
      { type: "item", text: "Soda (500ml)", qty: 3, price: "KES 150" },
      { type: "divider", text: "" },
      { type: "total", text: "TOTAL: KES 1,650" },
      { type: "footer", text: "Table 7 — Dine In" },
    ],
    cutPaper: true,
    priority: "normal",
    createdAt: new Date().toISOString(),
  };

  console.log(
    `[WS] Sending print_job to zone="${zone}": ${job.jobId}`
  );
  ws.send(JSON.stringify({ type: "print_job", job }));
}

console.log("\nPrintAgent Mock Server\n");
console.log("  To test pairing, set PRINT_AGENT_API_URL=http://localhost:3131");
console.log("  WS URL returned will be ws://localhost:8765");
console.log("\nWaiting for connections...\n");
