import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAgentStatus } from "../hooks/useAgentStatus";

interface Props {
  config: { agents: Array<{ agentId: string; zone: string; deviceName: string; branchId: string; printerType: string; printerHost?: string }> };
  onConfigChange: () => void;
  onShowSetup: () => void;
}

export default function StatusScreen({ config, onConfigChange, onShowSetup }: Props) {
  const { statuses, loading } = useAgentStatus();
  const [testPrinting, setTestPrinting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [expandedAgentId, setExpandedAgentId] = useState<string | null>(null);

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

  const handleUnpair = async (agentId: string) => {
    if (!confirm("Remove this agent?")) return;
    try {
      await invoke("unpair_device", { agentId });
      onConfigChange();
    } catch (e) {
      console.error("Failed to unpair:", e);
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
        <button
          onClick={onShowSetup}
          className="px-4 py-2 bg-green-500/20 hover:bg-green-500/30 text-green-400 rounded-lg text-sm font-medium transition-colors"
        >
          + Add Agent
        </button>
      </div>

      {/* Agents List */}
      <div className="flex-1 space-y-3">
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <div className="w-6 h-6 border-2 border-green-500 border-t-transparent rounded-full animate-spin" />
          </div>
        ) : config.agents.length === 0 ? (
          <div className="text-center py-12">
            <p className="text-gray-400">No agents configured</p>
          </div>
        ) : (
          config.agents.map((agent) => {
            const status = statuses.find(s => s.agentId === agent.agentId);
            const connected = status?.wsConnected ?? false;
            const queueCount = status?.queueCount ?? 0;

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
                          {connected ? "Connected" : "Disconnected"}
                        </span>
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
                      {agent.printerType === "network" && agent.printerHost && (
                        <div className="bg-gray-800/50 rounded-lg p-3">
                          <p className="text-gray-500 text-xs mb-1">Printer IP</p>
                          <p className="text-white text-sm">{agent.printerHost}</p>
                        </div>
                      )}
                    </div>
                    <div className="flex items-center justify-between">
                      <button
                        onClick={(e) => { e.stopPropagation(); handleUnpair(agent.agentId); }}
                        className="px-3 py-1.5 bg-red-500/10 hover:bg-red-500/20 text-red-400 rounded-lg text-sm font-medium transition-colors"
                      >
                        Remove Agent
                      </button>
                      <button
                        onClick={(e) => { e.stopPropagation(); handleTestPrint(agent.agentId); }}
                        disabled={testPrinting}
                        className="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors disabled:opacity-50"
                      >
                        Test Print
                      </button>
                    </div>
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
    </div>
  );
}
