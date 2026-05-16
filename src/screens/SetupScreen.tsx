import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onPaired: () => void;
}

export default function SetupScreen({ onPaired }: Props) {
  const [companyCode, setCompanyCode] = useState("");
  const [branchId, setBranchId] = useState("");
  const [zone, setZone] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const [printerPort, setPrinterPort] = useState("");
  const [availablePorts, setAvailablePorts] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [testPrinting, setTestPrinting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [portsLoading, setPortsLoading] = useState(false);

  const loadPorts = async () => {
    setPortsLoading(true);
    try {
      const ports = await invoke<string[]>("get_available_ports");
      setAvailablePorts(ports);
      if (ports.length > 0 && !printerPort) {
        setPrinterPort(ports[0]);
      }
    } catch (e) {
      setAvailablePorts([]);
    } finally {
      setPortsLoading(false);
    }
  };

  useEffect(() => {
    loadPorts();
  }, []);

  const handleTestPrint = async () => {
    if (!printerPort) {
      setError("Please select a printer port first.");
      return;
    }
    setTestPrinting(true);
    setError(null);
    setSuccess(null);
    try {
      await invoke("test_print");
      setSuccess("Test print sent successfully!");
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setTestPrinting(false);
    }
  };

  const handlePair = async () => {
    if (!companyCode.trim() || !branchId.trim() || !zone.trim() || !deviceName.trim() || !printerPort) {
      setError("All fields are required.");
      return;
    }
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      await invoke("pair_device", {
        companyCode: companyCode.trim(),
        branchId: branchId.trim(),
        zone: zone.trim(),
        deviceName: deviceName.trim(),
        printerPort,
      });
      setSuccess("Device paired successfully!");
      setTimeout(onPaired, 800);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-gray-950 flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        {/* Header */}
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-14 h-14 bg-green-500/10 border border-green-500/20 rounded-2xl mb-4">
            <svg className="w-7 h-7 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
                d="M6 9V2h12v7M6 18H4a2 2 0 01-2-2v-5a2 2 0 012-2h16a2 2 0 012 2v5a2 2 0 01-2 2h-2M6 14h12v8H6v-8z" />
            </svg>
          </div>
          <h1 className="text-2xl font-semibold text-white">PrintAgent Setup</h1>
          <p className="text-gray-400 text-sm mt-1">Configure this device to connect to your print network</p>
        </div>

        {/* Form Card */}
        <div className="bg-gray-900 border border-gray-800 rounded-2xl p-6 space-y-4">
          <Field label="Company Code" required>
            <input
              type="text"
              value={companyCode}
              onChange={(e) => setCompanyCode(e.target.value)}
              placeholder="e.g. ACME-2024"
              className="input-field"
            />
          </Field>

          <Field label="Branch ID" required>
            <input
              type="text"
              value={branchId}
              onChange={(e) => setBranchId(e.target.value)}
              placeholder="e.g. branch-nairobi-001"
              className="input-field"
            />
          </Field>

          <div className="grid grid-cols-2 gap-3">
            <Field label="Zone" required>
              <input
                type="text"
                value={zone}
                onChange={(e) => setZone(e.target.value)}
                placeholder="e.g. kitchen"
                className="input-field"
              />
            </Field>
            <Field label="Device Name" required>
              <input
                type="text"
                value={deviceName}
                onChange={(e) => setDeviceName(e.target.value)}
                placeholder="e.g. Kitchen Printer"
                className="input-field"
              />
            </Field>
          </div>

          <Field label="Printer Port" required>
            <div className="flex gap-2">
              <select
                value={printerPort}
                onChange={(e) => setPrinterPort(e.target.value)}
                className="input-field flex-1"
              >
                {portsLoading ? (
                  <option value="">Loading ports...</option>
                ) : availablePorts.length === 0 ? (
                  <option value="">No ports found</option>
                ) : (
                  availablePorts.map((p) => (
                    <option key={p} value={p}>{p}</option>
                  ))
                )}
              </select>
              <button
                onClick={loadPorts}
                disabled={portsLoading}
                className="px-3 py-2 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 text-sm transition-colors disabled:opacity-50"
                title="Refresh ports"
              >
                <svg className={`w-4 h-4 ${portsLoading ? "animate-spin" : ""}`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
              </button>
            </div>
          </Field>

          {/* Test Print */}
          <button
            onClick={handleTestPrint}
            disabled={!printerPort || testPrinting}
            className="w-full py-2 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors disabled:opacity-40 flex items-center justify-center gap-2"
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

          {/* Alerts */}
          {error && (
            <div className="flex items-start gap-2 bg-red-500/10 border border-red-500/20 text-red-400 rounded-lg p-3 text-sm">
              <svg className="w-4 h-4 mt-0.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {error}
            </div>
          )}
          {success && (
            <div className="flex items-start gap-2 bg-green-500/10 border border-green-500/20 text-green-400 rounded-lg p-3 text-sm">
              <svg className="w-4 h-4 mt-0.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {success}
            </div>
          )}

          {/* Pair Button */}
          <button
            onClick={handlePair}
            disabled={loading}
            className="w-full py-3 bg-green-600 hover:bg-green-500 disabled:bg-green-900 disabled:text-green-700 rounded-xl text-white font-semibold transition-colors flex items-center justify-center gap-2"
          >
            {loading ? (
              <>
                <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
                Pairing...
              </>
            ) : (
              "Pair Device"
            )}
          </button>
        </div>

        <p className="text-center text-gray-600 text-xs mt-4">
          PrintAgent v0.1.0 — Silent Print Routing
        </p>
      </div>
    </div>
  );
}

function Field({
  label,
  required,
  children,
}: {
  label: string;
  required?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1">
      <label className="text-xs font-medium text-gray-400 uppercase tracking-wide">
        {label}
        {required && <span className="text-green-500 ml-0.5">*</span>}
      </label>
      {children}
    </div>
  );
}
