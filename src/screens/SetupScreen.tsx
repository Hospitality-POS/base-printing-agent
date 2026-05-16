import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onPaired: () => void;
}

type PrinterType = "serial" | "network";

export default function SetupScreen({ onPaired }: Props) {
  const [companyCode, setCompanyCode] = useState("");
  const [branchId, setBranchId] = useState("");
  const [zone, setZone] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const [printerType, setPrinterType] = useState<PrinterType>("serial");
  const [printerPort, setPrinterPort] = useState("");
  const [printerHost, setPrinterHost] = useState("");
  const [printerNetworkPort, setPrinterNetworkPort] = useState("9100");
  const [networkPrinters, setNetworkPrinters] = useState<string[]>([]);
  const [scanning, setScanning] = useState(false);
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

  const scanNetworkPrinters = async () => {
    setScanning(true);
    setNetworkPrinters([]);
    setError(null);
    try {
      console.log("Calling discover_network_printers...");
      const found = await invoke<string[]>("discover_network_printers");
      console.log("Scan result:", found);
      setNetworkPrinters(found);
      if (found.length > 0 && !printerHost) {
        const first = found[0].split(" ")[0];
        setPrinterHost(first);
      }
      if (found.length === 0) setError("No printers found on port 9100. Enter the IP manually.");
    } catch (e) {
      console.error("Network scan error:", e);
      setError("Network scan failed: " + String(e));
    } finally {
      setScanning(false);
    }
  };

  useEffect(() => {
    loadPorts();
  }, []);

  const printerReady = printerType === "network"
    ? printerHost.trim().length > 0
    : printerPort.length > 0;

  const handleTestPrint = async () => {
    if (!printerReady) {
      setError(printerType === "network"
        ? "Enter a printer IP address first."
        : "Please select a printer port first.");
      return;
    }
    setTestPrinting(true);
    setError(null);
    setSuccess(null);
    try {
      await invoke("test_print", {
        printerType,
        printerPort: printerType === "serial" ? printerPort : null,
        printerHost: printerType === "network" ? printerHost.trim() : null,
        printerNetworkPort: printerType === "network" ? parseInt(printerNetworkPort) || 9100 : null,
      });
      setSuccess("Test print sent successfully!");
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setTestPrinting(false);
    }
  };

  const handlePair = async () => {
    if (!companyCode.trim() || !branchId.trim() || !zone.trim() || !deviceName.trim()) {
      setError("All fields are required.");
      return;
    }
    if (!printerReady) {
      setError(printerType === "network"
        ? "Enter a printer IP address."
        : "Select a printer port.");
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
        printerPort: printerType === "serial" ? printerPort : "",
        printerType,
        printerHost: printerType === "network" ? printerHost.trim() : null,
        printerNetworkPort: printerType === "network" ? parseInt(printerNetworkPort) || 9100 : null,
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
          <div className="inline-flex items-center justify-center mb-4">
            <img src="/logo.png" alt="Base" className="h-12 w-auto" />
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

          {/* Printer Connection Type Toggle */}
          <Field label="Printer Connection" required>
            <div className="flex rounded-lg border border-gray-700 overflow-hidden">
              {(["serial", "network"] as PrinterType[]).map((t) => (
                <button
                  key={t}
                  onClick={() => setPrinterType(t)}
                  className={`flex-1 py-2 text-sm font-medium transition-colors ${
                    printerType === t
                      ? "bg-green-600 text-white"
                      : "bg-gray-800 text-gray-400 hover:text-gray-200"
                  }`}
                >
                  {t === "serial" ? "USB / Serial" : "Network (TCP/IP)"}
                </button>
              ))}
            </div>
          </Field>

          {printerType === "serial" ? (
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
                    <option value="">No ports found — type manually below</option>
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
              <input
                type="text"
                value={printerPort}
                onChange={(e) => setPrinterPort(e.target.value)}
                placeholder="Or type manually, e.g. COM3 or /dev/usb/lp0"
                className="input-field mt-2"
              />
            </Field>
          ) : (
            <div className="space-y-3">
              {/* Scan button */}
              <button
                type="button"
                onClick={scanNetworkPrinters}
                disabled={scanning}
                className="w-full py-2.5 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
              >
                {scanning ? (
                  <>
                    <div className="w-4 h-4 border border-green-400 border-t-transparent rounded-full animate-spin" />
                    Scanning network for printers...
                  </>
                ) : (
                  <>
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0" />
                    </svg>
                    Scan Network for Printers
                  </>
                )}
              </button>

              {/* Discovered printers */}
              {networkPrinters.length > 0 && (
                <Field label="Discovered Printers" required>
                  <div className="space-y-1.5">
                    {networkPrinters.map((entry) => {
                      const ip = entry.split(" ")[0];
                      const selected = printerHost === ip;
                      return (
                        <button
                          key={ip}
                          type="button"
                          onClick={() => setPrinterHost(ip)}
                          className={`w-full flex items-center justify-between px-3 py-2.5 rounded-lg border text-sm transition-colors ${
                            selected
                              ? "bg-green-500/10 border-green-500/40 text-green-400"
                              : "bg-gray-800 border-gray-700 text-gray-300 hover:border-gray-600"
                          }`}
                        >
                          <div className="flex items-center gap-2">
                            <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 9V2h12v7M6 18H4a2 2 0 01-2-2v-5a2 2 0 012-2h16a2 2 0 012 2v5a2 2 0 01-2 2h-2M6 14h12v8H6v-8z" />
                            </svg>
                            <span className="font-mono">{ip}</span>
                          </div>
                          <span className="text-xs text-gray-500">port 9100</span>
                        </button>
                      );
                    })}
                  </div>
                </Field>
              )}

              {/* Manual fallback */}
              <Field label={networkPrinters.length > 0 ? "Or Enter IP Manually" : "Printer IP Address"} required={networkPrinters.length === 0}>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={printerHost}
                    onChange={(e) => setPrinterHost(e.target.value)}
                    placeholder="e.g. 192.168.1.100"
                    className="input-field flex-1"
                  />
                  <input
                    type="number"
                    value={printerNetworkPort}
                    onChange={(e) => setPrinterNetworkPort(e.target.value)}
                    placeholder="9100"
                    min={1}
                    max={65535}
                    className="input-field w-24"
                    title="TCP port (default 9100)"
                  />
                </div>
                <p className="text-xs text-gray-600 mt-1">IP : Port — default port 9100 (Epson, Star, Bixolon)</p>
              </Field>
            </div>
          )}

          {/* Test Print */}
          <button
            onClick={handleTestPrint}
            disabled={!printerReady || testPrinting}
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
