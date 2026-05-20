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

export default function StatusScreen({ config, shopId, onShowSetup }: Props) {
  const { statuses, loading } = useAgentStatus();
  const [agents, setAgents] = useState<Array<{
    agentId: string;
    zone: string;
    deviceName: string;
    branchId: string;
    companyCode: string;
    printerType: string;
    printerPort?: string;
    printerHost?: string;
    printerNetworkPort?: number;
  }>>([]);
  const [unpairConfirm, setUnpairConfirm] = useState<{ agentId: string | null }>({ agentId: null });
  const [unpairAllConfirm, setUnpairAllConfirm] = useState(false);

  // Fetch agents from API on mount
  React.useEffect(() => {
    const fetchAgents = async () => {
      try {
        const shopIdToUse = shopId || config.agents[0]?.branchId || "6a06e94aa2c6c63b9b080e00";
        console.log("Fetching agents from API with shopId:", shopIdToUse);
        const fetchedAgents = await invoke<any[]>("fetch_agents_from_api", { shopId: shopIdToUse, companyCode: config.agents[0]?.companyCode || "BP-358003" });
        console.log("Fetched agents from API:", fetchedAgents.length);
        setAgents(fetchedAgents.map((a: any) => ({
          agentId: a.agent_id,
          zone: a.zone,
          deviceName: a.device_name,
          branchId: a.shop_id,
          companyCode: a.company_code,
          printerType: a.printer_type || "serial",
          printerPort: a.printer_port,
          printerHost: a.printer_host,
          printerNetworkPort: a.printer_network_port,
        })));
      } catch (e) {
        console.error("Failed to fetch agents from API:", e);
        // Fall back to local config if API fails
        console.log("Falling back to local config, agents:", config.agents.length);
        setAgents(config.agents);
      }
    };
    fetchAgents();
  }, [config.agents]);

  React.useEffect(() => {
    console.log("StatusScreen mounted, agent count:", agents.length);
  }, [agents.length]);

  const [testPrinting, setTestPrinting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [expandedAgentId, setExpandedAgentId] = useState<string | null>(null);
  const [editingAgentId, setEditingAgentId] = useState<string | null>(null);
  const [testingConnection, setTestingConnection] = useState(false);
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

  const handleTestConnection = async (agent: any) => {
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

  const handleEditPrinter = (agent: any) => {
    setEditingAgentId(agent.agentId);
    setPrinterConfig({
      printerPort: agent.printerPort || "",
      printerType: agent.printerType || "serial",
      printerHost: agent.printerHost || "",
      printerNetworkPort: agent.printerNetworkPort?.toString() || "9100",
    });
    setConfigResult(null);
  };

  const handleSavePrinter = async () => {
    console.log("Save printer button clicked for agent:", editingAgentId);
    setSavingConfig(true);
    setConfigResult(null);
    try {
      const agent = agents.find(a => a.agentId === editingAgentId);
      console.log("Agent found:", agent);
      console.log("Printer config:", printerConfig);
      console.log("Calling update_agent_printer_port command");
      await invoke("update_agent_printer_port", {
        agentId: editingAgentId,
        printerPort: printerConfig.printerPort,
        printerType: printerConfig.printerType,
        printerHost: printerConfig.printerType === "network" ? printerConfig.printerHost : null,
        printerNetworkPort: printerConfig.printerType === "network" ? parseInt(printerConfig.printerNetworkPort) : null,
        companyCode: agent?.companyCode || "",
      });
      console.log("Printer config saved successfully");
      setConfigResult({ ok: true, msg: "Printer configuration saved to API!" });
      setEditingAgentId(null);
      // Refresh agents from API
      const shopIdToUse = shopId || agent?.branchId || "67841c7f52c7b8888375503b";
      const fetchedAgents = await invoke<any[]>("fetch_agents_from_api", { shopId: shopIdToUse });
      setAgents(fetchedAgents.map((a: any) => ({
        agentId: a.agent_id,
        zone: a.zone,
        deviceName: a.device_name,
        branchId: a.shop_id,
        companyCode: a.company_code,
        printerType: a.printer_type || "serial",
        printerPort: a.printer_port,
        printerHost: a.printer_host,
        printerNetworkPort: a.printer_network_port,
      })));
    } catch (e: unknown) {
      console.error("Failed to save printer config:", e);
      setConfigResult({ ok: false, msg: e instanceof Error ? e.message : String(e) });
    } finally {
      setSavingConfig(false);
      setTimeout(() => setConfigResult(null), 4000);
    }
  };

  const handleUnpair = async (agentId: string) => {
    setUnpairConfirm({ agentId });
  };

  const confirmUnpair = async () => {
    const agentId = unpairConfirm.agentId;
    if (!agentId) return;
    
    console.log("Calling unpair_device command");
    try {
      await invoke("unpair_device", { agentId });
      console.log("Unpair successful");
      // Refresh agents from API
      const shopIdToUse = shopId || "67841c7f52c7b8888375503b";
      const fetchedAgents = await invoke<any[]>("fetch_agents_from_api", { shopId: shopIdToUse });
      setAgents(fetchedAgents.map((a: any) => ({
        agentId: a.agent_id,
        zone: a.zone,
        deviceName: a.device_name,
        branchId: a.shop_id,
        companyCode: a.company_code,
        printerType: a.printer_type || "serial",
        printerPort: a.printer_port,
        printerHost: a.printer_host,
        printerNetworkPort: a.printer_network_port,
      })));
      setUnpairConfirm({ agentId: null });
    } catch (e: unknown) {
      console.error("Failed to unpair:", e);
    }
  };

  const handleUnpairAll = async () => {
    setUnpairAllConfirm(true);
  };

  const confirmUnpairAll = async () => {
    try {
      const result = await invoke<string>("unpair_all_agents");
      alert(result);
      setAgents([]);
      setUnpairAllConfirm(false);
    } catch (e: unknown) {
      console.error("Failed to unpair all agents:", e);
      alert("Failed to unpair all agents: " + (e instanceof Error ? e.message : String(e)));
    }
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
            onClick={handleUnpairAll}
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
            // Agent is considered "connected" if it has WebSocket credentials OR if it's polling (has branch_id and zone)
            const isPolling = !wsConnected && agent.branchId && agent.zone;
            const connected = wsConnected || isPolling;
            // Check if printer configuration is valid
            const hasValidPrinter = agent.printerType === "network"
              ? !!(agent.printerHost && agent.printerNetworkPort)
              : !!(agent.printerPort);

            return (
              <div key={agent.agentId} className="bg-gray-900 rounded-2xl border border-gray-800 overflow-hidden">
                <div 
                  className="p-4 cursor-pointer hover:bg-gray-800/50 transition-colors"
                  onClick={() => setExpandedAgentId(expandedAgentId === agent.agentId ? null : agent.agentId)}
                >
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex-1">
                      <div className="flex items-center gap-2 mb-1">
                        <h3 className="text-white font-medium">{agent.deviceName}</h3>
                        <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${
                          connected
                            ? "bg-green-500/20 text-green-400"
                            : "bg-red-500/20 text-red-400"
                        }`}>
                          {isPolling ? "Polling" : wsConnected ? "Connected" : "Disconnected"}
                        </span>
                        {!hasValidPrinter && (
                          <span className="px-2 py-0.5 rounded-full text-xs font-medium bg-yellow-500/20 text-yellow-400">
                            Configure Printer
                          </span>
                        )}
                      </div>
                      <p className="text-gray-400 text-sm">{agent.zone} • {agent.agentId.slice(0, 8)}...</p>
                    </div>
                    <div className="flex items-center gap-2">
                      <svg className={`w-4 h-4 text-gray-500 transition-transform ${expandedAgentId === agent.agentId ? 'rotate-180' : ''}`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                      </svg>
                    </div>
                  </div>

                  <div className="flex items-center justify-between text-sm">
                    <div className="flex items-center gap-4">
                      <div className="flex items-center gap-1.5 text-gray-400">
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
                        </svg>
                        Queue: {queueCount}
                      </div>
                    </div>
                  </div>
                </div>

                {expandedAgentId === agent.agentId && (
                  <div className="p-4 pt-0 border-t border-gray-800">
                    {editingAgentId === agent.agentId ? (
                      <div className="space-y-3 mb-3">
                        <div>
                          <label className="text-gray-500 text-xs mb-1 block">Printer Type</label>
                          <select
                            value={printerConfig.printerType}
                            onChange={(e) => setPrinterConfig({ ...printerConfig, printerType: e.target.value })}
                            className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm"
                          >
                            <option value="serial">Serial</option>
                            <option value="network">Network</option>
                          </select>
                        </div>
                        {printerConfig.printerType === "serial" ? (
                          <div>
                            <label className="text-gray-500 text-xs mb-1 block">Printer Port</label>
                            <input
                              type="text"
                              value={printerConfig.printerPort}
                              onChange={(e) => setPrinterConfig({ ...printerConfig, printerPort: e.target.value })}
                              placeholder="/dev/tty.usbserial-xxx"
                              className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm"
                            />
                          </div>
                        ) : (
                          <>
                            <div>
                              <label className="text-gray-500 text-xs mb-1 block">Printer Host/IP</label>
                              <input
                                type="text"
                                value={printerConfig.printerHost}
                                onChange={(e) => setPrinterConfig({ ...printerConfig, printerHost: e.target.value })}
                                placeholder="192.168.1.100"
                                className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-white text-sm"
                              />
                            </div>
                            <div>
                              <label className="text-gray-500 text-xs mb-1 block">Printer Port</label>
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
                        <div className="flex items-center gap-2">
                          <button
                            onClick={handleSavePrinter}
                            disabled={savingConfig}
                            className="flex-1 px-3 py-1.5 bg-green-500/20 hover:bg-green-500/30 text-green-400 rounded-lg text-sm font-medium transition-colors disabled:opacity-50"
                          >
                            {savingConfig ? "Saving..." : "Save to API"}
                          </button>
                          <button
                            onClick={() => setEditingAgentId(null)}
                            className="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors"
                          >
                            Cancel
                          </button>
                        </div>
                        {configResult && (
                          <div className={`p-2 rounded-lg text-xs ${
                            configResult.ok
                              ? "bg-green-500/10 text-green-400"
                              : "bg-red-500/10 text-red-400"
                          }`}>
                            {configResult.msg}
                          </div>
                        )}
                      </div>
                    ) : (
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
                          {agent.printerType === "network" && agent.printerHost ? (
                            <div className="bg-gray-800/50 rounded-lg p-3">
                              <p className="text-gray-500 text-xs mb-1">Printer IP</p>
                              <p className="text-white text-sm">{agent.printerHost}</p>
                            </div>
                          ) : (
                            <div className="bg-gray-800/50 rounded-lg p-3">
                              <p className="text-gray-500 text-xs mb-1">Printer Port</p>
                              <p className="text-white text-sm truncate">{agent.printerPort || "Not configured"}</p>
                            </div>
                          )}
                        </div>
                        <div className="flex items-center justify-between">
                          <button
                            onClick={() => handleUnpair(agent.agentId)}
                            className="px-3 py-1.5 bg-red-500/10 hover:bg-red-500/20 text-red-400 rounded-lg text-sm font-medium transition-colors"
                          >
                            Remove Agent
                          </button>
                          <div className="flex items-center gap-2">
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

      {/* Unpair Confirmation Modal */}
      {unpairConfirm.agentId && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-gray-900 border border-gray-800 rounded-2xl p-6 max-w-sm w-full mx-4">
            <h3 className="text-lg font-semibold text-white mb-2">Remove Agent</h3>
            <p className="text-gray-400 text-sm mb-6">
              Are you sure you want to remove agent {unpairConfirm.agentId}? This will disconnect the printer from this device.
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
