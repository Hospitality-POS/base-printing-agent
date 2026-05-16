import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAgentStatus } from "../hooks/useAgentStatus";

interface Props {
  onUnpair: () => void;
}

export default function StatusScreen({ onUnpair }: Props) {
  const { status, loading } = useAgentStatus();
  const [testPrinting, setTestPrinting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [unpairing, setUnpairing] = useState(false);

  const handleTestPrint = async () => {
    setTestPrinting(true);
    setTestResult(null);
    try {
      await invoke("test_print");
      setTestResult({ ok: true, msg: "Test print sent!" });
    } catch (e: unknown) {
      setTestResult({ ok: false, msg: e instanceof Error ? e.message : String(e) });
    } finally {
      setTestPrinting(false);
      setTimeout(() => setTestResult(null), 4000);
    }
  };

  const handleUnpair = async () => {
    if (!confirm("This will remove the device pairing. Are you sure?")) return;
    setUnpairing(true);
    try {
      await invoke("unpair_device");
      onUnpair();
    } catch (e) {
      setUnpairing(false);
    }
  };

  const wsConnected = status?.wsConnected ?? false;

  return (
    <div className="min-h-screen bg-gray-950 p-4 flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 bg-green-500/10 border border-green-500/20 rounded-xl flex items-center justify-center">
            <svg className="w-5 h-5 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
                d="M6 9V2h12v7M6 18H4a2 2 0 01-2-2v-5a2 2 0 012-2h16a2 2 0 012 2v5a2 2 0 01-2 2h-2M6 14h12v8H6v-8z" />
            </svg>
          </div>
          <div>
            <h1 className="text-sm font-semibold text-white">PrintAgent</h1>
            <p className="text-xs text-gray-500">Silent Print Routing</p>
          </div>
        </div>

        {/* WS Status Badge */}
        <div className={`flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium border ${
          wsConnected
            ? "bg-green-500/10 border-green-500/20 text-green-400"
            : "bg-red-500/10 border-red-500/20 text-red-400"
        }`}>
          <span className={`w-1.5 h-1.5 rounded-full ${wsConnected ? "bg-green-400 animate-pulse" : "bg-red-400"}`} />
          {wsConnected ? "Connected" : "Disconnected"}
        </div>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-2 gap-3 mb-4">
        <StatCard
          icon={
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17.657 16.657L13.414 20.9a1.998 1.998 0 01-2.827 0l-4.244-4.243a8 8 0 1111.314 0z" />
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 11a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          }
          label="Zone"
          value={loading ? "—" : (status?.zone || "—")}
          color="blue"
        />
        <StatCard
          icon={
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-2 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4" />
            </svg>
          }
          label="Branch"
          value={loading ? "—" : (status?.branchId || "—")}
          color="purple"
        />
        <StatCard
          icon={
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
            </svg>
          }
          label="Queue"
          value={loading ? "—" : String(status?.queueCount ?? 0)}
          color={status?.queueCount ? "yellow" : "gray"}
        />
        <StatCard
          icon={
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          }
          label="Last Job"
          value={loading ? "—" : (status?.lastJobId ? status.lastJobId.slice(0, 8) + "..." : "None")}
          color="green"
        />
      </div>

      {/* Last Job Detail */}
      {status?.lastJobId && (
        <div className="bg-gray-900 border border-gray-800 rounded-xl p-3 mb-4">
          <p className="text-xs text-gray-500 mb-1">Last Print Job</p>
          <p className="text-xs font-mono text-gray-300">{status.lastJobId}</p>
          {status.lastJobTime && (
            <p className="text-xs text-gray-500 mt-1">{new Date(status.lastJobTime).toLocaleString()}</p>
          )}
        </div>
      )}

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

      <div className="mt-auto space-y-2">
        {/* Test Print */}
        <button
          onClick={handleTestPrint}
          disabled={testPrinting}
          className="w-full py-2.5 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-xl text-gray-300 text-sm font-medium transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
        >
          {testPrinting ? (
            <>
              <div className="w-4 h-4 border border-gray-400 border-t-transparent rounded-full animate-spin" />
              Printing...
            </>
          ) : (
            <>
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 17h2a2 2 0 002-2v-4a2 2 0 00-2-2H5a2 2 0 00-2 2v4a2 2 0 002 2h2m2 4h6a2 2 0 002-2v-4a2 2 0 00-2-2H9a2 2 0 00-2 2v4a2 2 0 002 2zm8-12V5a2 2 0 00-2-2H9a2 2 0 00-2 2v4h10z" />
              </svg>
              Test Print
            </>
          )}
        </button>

        {/* Unpair */}
        <button
          onClick={handleUnpair}
          disabled={unpairing}
          className="w-full py-2.5 bg-transparent hover:bg-red-500/10 border border-gray-800 hover:border-red-500/20 rounded-xl text-gray-500 hover:text-red-400 text-sm font-medium transition-colors flex items-center justify-center gap-2"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
          </svg>
          Disconnect / Re-pair
        </button>
      </div>
    </div>
  );
}

function StatCard({
  icon,
  label,
  value,
  color,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  color: "green" | "blue" | "purple" | "yellow" | "gray";
}) {
  const colorMap = {
    green: "text-green-400 bg-green-500/10",
    blue: "text-blue-400 bg-blue-500/10",
    purple: "text-purple-400 bg-purple-500/10",
    yellow: "text-yellow-400 bg-yellow-500/10",
    gray: "text-gray-400 bg-gray-800",
  };

  return (
    <div className="bg-gray-900 border border-gray-800 rounded-xl p-3">
      <div className={`inline-flex items-center justify-center w-7 h-7 rounded-lg mb-2 ${colorMap[color]}`}>
        {icon}
      </div>
      <p className="text-xs text-gray-500 mb-0.5">{label}</p>
      <p className="text-sm font-semibold text-white truncate">{value}</p>
    </div>
  );
}
