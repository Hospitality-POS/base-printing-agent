import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAgentStatus } from "../hooks/useAgentStatus";

interface Config {
  agents: Array<{
    agentId: string;
    zone: string;
    deviceName: string;
    branchId: string;
    companyCode: string;
    printerType: string;
    printerPort?: string;
    printerHost?: string;
    printerNetworkPort?: number;
  }>;
}

interface Props {
  config: Config;
  shopId: string;
  onShowSetup: () => void;
}

interface Agent {
  agentId: string;
  zone: string;
  deviceName: string;
  branchId: string;
  companyCode: string;
  printerType: string;
  printerPort?: string;
  printerHost?: string;
  printerNetworkPort?: number;
}

export default function StatusScreen({ config, shopId, onShowSetup }: Props) {
  const { statuses, loading } = useAgentStatus();
  const [agents, setAgents] = useState<Agent[]>([]);
  const [unpairConfirm, setUnpairConfirm] = useState<{ agentId: string | null }>({ agentId: null });
  const [unpairAllConfirm, setUnpairAllConfirm] = useState(false);
  const [testPrinting, setTestPrinting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [expandedAgentId, setExpandedAgentId] = useState<string | null>(null);
  const [editingAgentId, setEditingAgentId] = useState<string | null>(null);
  const [testingConnection, setTestingConnection] = useState(false);
  const [windowsPrinters, setWindowsPrinters] = useState<string[]>([]);
  const [loadingWindowsPrinters, setLoadingWindowsPrinters] = useState(false);
  const [printerConfig, setPrinterConfig] = useState<{
    printerPort: string;
    printerType: string;
    printerHost: string;
    printerNetworkPort: string;
  }>({
    printerPort: "",
    printerType: "serial",
    printerHost: "",
    printerNetworkPort: "9100",
  });
  const [savingConfig, setSavingConfig] = useState(false);
  const [configResult, setConfigResult] = useState<{ ok: boolean; msg: string } | null>(null);

  const mapAgent = (a: any): Agent => ({
    agentId: a.agent_id,
    zone: a.zone,
    deviceName: a.device_name,
    branchId: a.shop_id,
    companyCode: a.company_code,
    printerType: a.printer_type || "serial",
    printerPort: a.printer_port,
    printerHost: a.printer_host,
    printerNetworkPort: a.printer_network_port,
  });

  const fetchAgents = async (overrideShopId?: string) => {
    try {
      const shopIdToUse = overrideShopId || shopId || config.agents[0]?.branchId || "";
      const companyCode = config.agents[0]?.companyCode || "";
      const fetched = await invoke<any[]>("fetch_agents_from_api", {
        shopId: shopIdToUse,
        companyCode,
      });
      setAgents(fetched.map(mapAgent));
    } catch (e) {
      console.error("Failed to fetch agents from API:", e);
      setAgents(config.agents);
    }
  };

  React.useEffect(() => {
    fetchAgents();
  }, [config.agents]);

  const loadWindowsPrinters = async () => {
    setLoadingWindowsPrinters(true);
    try {
      const printers = await invoke<string[]>("list_windows_printers");
      setWindowsPrinters(printers);
      if (printers.length === 0) {
        setConfigResult({ ok: false, msg: "No Windows printers found. Make sure your printer driver is installed." });
      }
    } catch (e) {
      setWindowsPrinters([]);
      setConfigResult({ ok: false, msg: "Failed to load Windows printers: " + String(e) });
    } finally {
      setLoadingWindowsPrinters(false);
    }
  };

  const handleTestPrint = async (agentId: string) => {
    setTestPrinting(true);
    setTestResult(null);
    try {
      await invoke("test_print", { agentId });
      setTestResult({ ok: true, msg: "Test print sent!" });
    } catch (e: unknown) {
      setTestResult({ ok: false, msg: e instanceof Error ? e.message : String(e) });
    } finally {
      setTestPrinting(false);
      setTimeout(() => setTestResult(null), 4000);
    }
  };

  const handleTestConnection = async (agent: Agent) => {
    if (agent.printerType !== "network" || !agent.printerHost) {
      alert("Connection test only works for network printers with configured IP");
      return;
    }
    setTestingConnection(true);
    try {
      const result = await invoke<string>("test_printer_connection", {
        printerHost: agent.printerHost,
        printerNetworkPort: agent.printerNetworkPort || 9100,
      });
      alert(result);
    } catch (e: unknown) {
      alert("Connection failed: " + (e instanceof Error ? e.message : String(e)));
    } finally {
      setTestingConnection(false);
    }
  };

  const handleEditPrinter = (agent: Agent) => {
    setEditingAgentId(agent.agentId);
    setWindowsPrinters([]);
    setPrinterConfig({
      printerPort: agent.printerPort || "",
      printerType: agent.printerType || "serial",
      printerHost: agent.printerHost || "",
      printerNetworkPort: agent.printerNetworkPort?.toString() || "9100",
    });
    setConfigResult(null);
    // Auto-load Windows printers if agent is already configured as windows type
    if (agent.printerType === "windows") {
      loadWindowsPrinters();
    }
  };

  const handleSavePrinter = async () => {
    setSavingConfig(true);
    setConfigResult(null);
    try {
      const agent = agents.find(a => a.agentId === editingAgentId);
      await invoke("update_agent_printer_port", {
        agentId: editingAgentId,
        printerPort: printerConfig.printerPort,
        printerType: printerConfig.printerType,
        printerHost: printerConfig.printerType === "network" ? printerConfig.printerHost : null,
        printerNetworkPort: printerConfig.printerType === "network"
          ? parseInt(printerConfig.printerNetworkPort)
          : null,
        companyCode: agent?.companyCode || "",
      });
      setConfigResult({ ok: true, msg: "Printer configuration saved!" });
      setEditingAgentId(null);
      await fetchAgents(agent?.branchId);
    } catch (e: unknown) {
      setConfigResult({ ok: false, msg: e instanceof Error ? e.message : String(e) });
    } finally {
      setSavingConfig(false);
      setTimeout(() => setConfigResult(null), 4000);
    }
  };

  const confirmUnpair = async () => {
    const agentId = unpairConfirm.agentId;
    if (!agentId) return;
    try {
      await invoke("unpair_device", { agentId });
      await fetchAgents();
      setUnpairConfirm({ agentId: null });
    } catch (e: unknown) {
      console.error("Failed to unpair:", e);
    }
  };

  const confirmUnpairAll = async () => {
    try {
      const result = await invoke<string>("unpair_all_agents");
      alert(result);
      setAgents([]);
      setUnpairAllConfirm(false);
    } catch (e: unknown) {
      alert("Failed to unpair all agents: " + (e instanceof Error ? e.message : String(e)));
    }
  };

  const hasValidPrinter = (agent: Agent) => {
    if (agent.printerType === "network") return !!(agent.printerHost && agent.printerNetworkPort);
    if (agent.printerType === "windows") return !!(agent.printerPort);
    return !!(agent.printerPort);
  };

  return (
    <div className="min-h-screen bg-gray-950 p-4 flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 bg-green-500/20 rounded-xl flex items-center justify-center">
            <svg className="w-5 h-5 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          </div>
          <div>
            <h1 className="text-white font-semibold">PrintAgent</h1>
            <p className="text-gray-400 text-sm">Connected Agents</p>
          </div>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={() => setUnpairAllConfirm(true)}
            className="px-4 py-2 bg-red-500/20 hover:bg-red-500/30 text-red-400 rounded-lg text-sm font-medium transition-colors"
          >
            Unpair All
          </button>
          <button
            onClick={onShowSetup}
            className="px-4 py-2 bg-green-500/20 hover:bg-green-500/30 text-green-400 rounded-lg text-sm font-medium transition-colors"
          >
            + Add Agent
          </button>
        </div>
      </div>

      {/* Agents List */}
      <div className="flex-1 space-y-3">
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <div className="w-6 h-6 border-2 border-green-500 border-t-transparent rounded-full animate-spin" />
          </div>
        ) : agents.length === 0 ? (
          <div className="text-center py-8 text-gray-400">No agents connected</div>
        ) : (
          agents.map((agent) => {
            const status = statuses.find(s => s.agentId === agent.agentId);
            const wsConnected = status?.wsConnected ?? false;
            const queueCount = status?.queueCount ?? 0;
            const isPolling = !wsConnected && agent.branchId && agent.zone;
            const connected = wsConnected || isPolling;
            const validPrinter = hasValidPrinter(agent);

            return (
              <div key={agent.agentId} className="bg-gray-900 rounded-2xl border border-gray-800 overflow-hidden">
                {/* Agent Header Row */}
                <div
                  className="p-4 cursor-pointer hover:bg-gray-800/50 transition-colors"
                  onClick={() => setExpandedAgentId(expandedAgentId === agent.agentId ? null : agent.agentId)}
                >
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex-1">
                      <div className="flex items-center gap-2 mb-1 flex-wrap">
                        <h3 className="text-white font-medium">{agent.deviceName}</h3>
                        <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${
                          connected ? "bg-green-500/20 text-green-400" : "bg-red-500/20 text-red-400"
                        }`}>
                          {isPolling ? "Polling" : wsConnected ? "Connected" : "Disconnected"}
                        </span>
                        {!validPrinter && (
                          <span className="px-2 py-0.5 rounded-full text-xs font-medium bg-yellow-500/20 text-yellow-400">
                            Configure Printer
                          </span>
                        )}
                      </div>
                      <p className="text-gray-400 text-sm">{agent.zone} • {agent.agentId.slice(0, 8)}...</p>
                    </div>
                    <svg
                      className={`w-4 h-4 text-gray-500 transition-transform ${expandedAgentId === agent.agentId ? "rotate-180" : ""}`}
                      fill="none" viewBox="0 0 24 24" stroke="currentColor"
                    >
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                    </svg>
                  </div>
                  <div className="flex items-center gap-1.5 text-gray-400 text-sm">
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
                    </svg>
                    Queue: {queueCount}
                  </div>
                </div>

                {/* Expanded Panel */}
                {expandedAgentId === agent.agentId && (
                  <div className="p-4 pt-0 border-t border-gray-800">

                    {/* Edit Form */}
                    {editingAgentId === agent.agentId ? (
                      <div className="space-y-3 mb-3">

                        {/* Printer Type */}
                        <div>
                          <label className="text-gray-500 text-xs mb-1 block">Printer Type</label>
                          <select
                            value={printerConfig.printerType}
                            onChange={(e) => {
                              const t = e.target.value;
                              setPrinterConfig({ ...printerConfig, printerType: t, printerPort: "" });
                              setWindowsPrinters([]);
                              if (t === "windows") loadWindowsPrinters();
                            }}
                            className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm"
                          >
                            <option value="serial">Serial / USB (COM port)</option>
                            <option value="network">Network (TCP/IP)</option>
                            <option value="windows">Windows Printer</option>
                          </select>
                        </div>

                        {/* Serial */}
                        {printerConfig.printerType === "serial" && (
                          <div>
                            <label className="text-gray-500 text-xs mb-1 block">Printer Port</label>
                            <input
                              type="text"
                              value={printerConfig.printerPort}
                              onChange={(e) => setPrinterConfig({ ...printerConfig, printerPort: e.target.value })}
                              placeholder="e.g. COM3 or /dev/ttyUSB0"
                              className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm"
                            />
                          </div>
                        )}

                        {/* Network */}
                        {printerConfig.printerType === "network" && (
                          <>
                            <div>
                              <label className="text-gray-500 text-xs mb-1 block">Printer Host / IP</label>
                              <input
                                type="text"
                                value={printerConfig.printerHost}
                                onChange={(e) => setPrinterConfig({ ...printerConfig, printerHost: e.target.value })}
                                placeholder="192.168.1.100"
                                className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm"
                              />
                            </div>
                            <div>
                              <label className="text-gray-500 text-xs mb-1 block">TCP Port</label>
                              <input
                                type="text"
                                value={printerConfig.printerNetworkPort}
                                onChange={(e) => setPrinterConfig({ ...printerConfig, printerNetworkPort: e.target.value })}
                                placeholder="9100"
                                className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm"
                              />
                            </div>
                          </>
                        )}

                        {/* Windows Printer */}
                        {printerConfig.printerType === "windows" && (
                          <div>
                            <div className="flex items-center justify-between mb-1">
                              <label className="text-gray-500 text-xs">Windows Printer Name</label>
                              <button
                                onClick={loadWindowsPrinters}
                                disabled={loadingWindowsPrinters}
                                className="text-xs text-blue-400 hover:text-blue-300 disabled:opacity-50"
                              >
                                {loadingWindowsPrinters ? "Loading..." : "↻ Refresh"}
                              </button>
                            </div>

                            {windowsPrinters.length > 0 ? (
                              <div className="space-y-1 mb-2">
                                {windowsPrinters.map((name) => (
                                  <div key={name} className="flex gap-2">
                                    <button
                                      type="button"
                                      onClick={() => setPrinterConfig({ ...printerConfig, printerPort: name })}
                                      className={`flex-1 px-3 py-2 text-left text-sm rounded-lg border transition-colors ${
                                        printerConfig.printerPort === name
                                          ? "bg-green-600/20 border-green-500 text-green-400"
                                          : "bg-gray-800 border-gray-700 text-gray-300 hover:bg-gray-700"
                                      }`}
                                    >
                                      {name}
                                    </button>
                                    <button
                                      type="button"
                                      onClick={async () => {
                                        try {
                                          await invoke("test_print", {
                                            printerType: "windows",
                                            printerPort: name,
                                          });
                                          setConfigResult({ ok: true, msg: `Test print sent to ${name}` });
                                        } catch (e: unknown) {
                                          setConfigResult({ ok: false, msg: e instanceof Error ? e.message : String(e) });
                                        }
                                      }}
                                      className="px-3 py-2 text-sm bg-gray-800 border border-gray-700 rounded-lg text-gray-300 hover:bg-gray-700 transition-colors"
                                    >
                                      Test
                                    </button>
                                  </div>
                                ))}
                              </div>
                            ) : (
                              <p className="text-xs text-gray-500 mb-2">
                                {loadingWindowsPrinters ? "Loading..." : "Click Refresh to load installed Windows printers"}
                              </p>
                            )}

                            {/* Manual entry fallback */}
                            <input
                              type="text"
                              value={printerConfig.printerPort}
                              onChange={(e) => setPrinterConfig({ ...printerConfig, printerPort: e.target.value })}
                              placeholder="Or type printer name manually"
                              className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm"
                            />
                            <p className="text-xs text-gray-500 mt-1">
                              Must match exactly the name in Windows → Printers & Scanners
                            </p>
                          </div>
                        )}

                        {/* Save / Cancel */}
                        <div className="flex items-center gap-2">
                          <button
                            onClick={handleSavePrinter}
                            disabled={savingConfig}
                            className="flex-1 px-3 py-1.5 bg-green-500/20 hover:bg-green-500/30 text-green-400 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
                          >
                            {savingConfig ? "Saving..." : "Save to API"}
                          </button>
                          <button
                            onClick={() => { setEditingAgentId(null); setWindowsPrinters([]); }}
                            className="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors"
                          >
                            Cancel
                          </button>
                        </div>

                        {configResult && (
                          <div className={`p-2 rounded-lg text-xs ${
                            configResult.ok ? "bg-green-500/10 text-green-400" : "bg-red-500/10 text-red-400"
                          }`}>
                            {configResult.msg}
                          </div>
                        )}
                      </div>

                    ) : (
                      /* Info View */
                      <>
                        <div className="grid grid-cols-2 gap-3 mb-3">
                          <div className="bg-gray-800/50 rounded-lg p-3">
                            <p className="text-gray-500 text-xs mb-1">Branch ID</p>
                            <p className="text-white text-sm truncate">{agent.branchId}</p>
                          </div>
                          <div className="bg-gray-800/50 rounded-lg p-3">
                            <p className="text-gray-500 text-xs mb-1">Zone</p>
                            <p className="text-white text-sm">{agent.zone}</p>
                          </div>
                          <div className="bg-gray-800/50 rounded-lg p-3">
                            <p className="text-gray-500 text-xs mb-1">Printer Type</p>
                            <p className="text-white text-sm capitalize">{agent.printerType}</p>
                          </div>
                          <div className="bg-gray-800/50 rounded-lg p-3">
                            <p className="text-gray-500 text-xs mb-1">
                              {agent.printerType === "network" ? "Printer IP" : "Printer Port / Name"}
                            </p>
                            <p className="text-white text-sm truncate">
                              {agent.printerType === "network"
                                ? agent.printerHost || "Not configured"
                                : agent.printerPort || "Not configured"}
                            </p>
                          </div>
                        </div>

                        <div className="flex items-center justify-between flex-wrap gap-2">
                          <button
                            onClick={() => setUnpairConfirm({ agentId: agent.agentId })}
                            className="px-3 py-1.5 bg-red-500/10 hover:bg-red-500/20 text-red-400 rounded-lg text-sm font-medium transition-colors"
                          >
                            Remove Agent
                          </button>
                          <div className="flex items-center gap-2 flex-wrap">
                            {agent.printerType === "network" && agent.printerHost && (
                              <button
                                onClick={() => handleTestConnection(agent)}
                                disabled={testingConnection}
                                className="px-3 py-1.5 bg-purple-500/10 hover:bg-purple-500/20 text-purple-400 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
                              >
                                {testingConnection ? "Testing..." : "Test Connection"}
                              </button>
                            )}
                            <button
                              onClick={() => handleEditPrinter(agent)}
                              className="px-3 py-1.5 bg-blue-500/10 hover:bg-blue-500/20 text-blue-400 rounded-lg text-sm font-medium transition-colors"
                            >
                              Configure Printer
                            </button>
                            <button
                              onClick={() => handleTestPrint(agent.agentId)}
                              disabled={testPrinting}
                              className="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors disabled:opacity-50"
                            >
                              Test Print
                            </button>
                          </div>
                        </div>
                      </>
                    )}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* Test Result Alert */}
      {testResult && (
        <div className={`flex items-center gap-2 p-3 rounded-xl text-sm mb-4 border ${
          testResult.ok
            ? "bg-green-500/10 border-green-500/20 text-green-400"
            : "bg-red-500/10 border-red-500/20 text-red-400"
        }`}>
          {testResult.ok ? (
            <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
          ) : (
            <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          )}
          {testResult.msg}
        </div>
      )}

      {/* Unpair Single Confirmation Modal */}
      {unpairConfirm.agentId && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-gray-900 border border-gray-800 rounded-2xl p-6 max-w-sm w-full mx-4">
            <h3 className="text-lg font-semibold text-white mb-2">Remove Agent</h3>
            <p className="text-gray-400 text-sm mb-6">
              Are you sure you want to remove this agent? This will disconnect the printer from this device.
            </p>
            <div className="flex gap-3">
              <button
                onClick={() => setUnpairConfirm({ agentId: null })}
                className="flex-1 px-4 py-2 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={confirmUnpair}
                className="flex-1 px-4 py-2 bg-red-600 hover:bg-red-700 rounded-lg text-white text-sm font-medium transition-colors"
              >
                Remove
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Unpair All Confirmation Modal */}
      {unpairAllConfirm && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-gray-900 border border-gray-800 rounded-2xl p-6 max-w-sm w-full mx-4">
            <h3 className="text-lg font-semibold text-white mb-2">Remove All Agents</h3>
            <p className="text-gray-400 text-sm mb-6">
              Are you sure you want to remove all agents? This will clear all local configurations and disconnect all printers.
            </p>
            <div className="flex gap-3">
              <button
                onClick={() => setUnpairAllConfirm(false)}
                className="flex-1 px-4 py-2 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={confirmUnpairAll}
                className="flex-1 px-4 py-2 bg-red-600 hover:bg-red-700 rounded-lg text-white text-sm font-medium transition-colors"
              >
                Remove All
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}