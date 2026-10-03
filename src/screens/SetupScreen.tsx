import { useEffect, useState } from "react";
import { invoke } from "../lib/invoke";

interface Props {
  onPaired: (branchId: string) => void;
}

type PrinterType = "serial" | "network" | "windows";

interface Shop {
  _id: string;
  name: string;
  shop_code?: string;
}

interface PortInfo {
  name: string;
  port_type: {
    type_name: string;
    vid?: number;
    pid?: number;
    manufacturer?: string;
    product?: string;
  };
}

interface ConnectedAgent {
  _id: string;
  agent_id: string;
  company_code: string;
  company_id: string;
  shop_id: string;
  zone: string;
  device_name: string;
  is_active: boolean;
  last_seen: string;
  createdAt: string;
  updatedAt: string;
  connected: boolean;
  printer_port?: string;
  printer_type?: string;
  printer_host?: string;
  printer_network_port?: number;
}

interface ConnectedAgentsResponse {
  agents: ConnectedAgent[];
  count: number;
}

interface OsInfo {
  major: number;
  minor: number;
  build: number;
  name: string;
  supportsDesktopApp: boolean;
  supportsHelper: boolean;
  hostname?: string;
}

interface SelectOption {
  _id: string;
  name: string;
}

/// Compact multi-select: selected items render as removable chips inside the
/// field; the option list opens as a floating dropdown so it never pushes
/// page content down.
function OptionMultiSelect({
  options,
  selectedIds,
  onChange,
  placeholder,
}: {
  options: SelectOption[];
  selectedIds: string[];
  onChange: (ids: string[], names: string[]) => void;
  placeholder: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const selected = options.filter((o) => selectedIds.includes(o._id));
  const filtered = query
    ? options.filter((o) => o.name.toLowerCase().includes(query.toLowerCase()))
    : options;

  const emit = (ids: string[]) =>
    onChange(ids, options.filter((o) => ids.includes(o._id)).map((o) => o.name));

  const toggle = (id: string) =>
    emit(selectedIds.includes(id) ? selectedIds.filter((i) => i !== id) : [...selectedIds, id]);

  return (
    <div className="relative">
      <div
        className="input-field h-[38px] flex items-center gap-1.5 cursor-pointer overflow-hidden"
        onClick={() => setOpen((o) => !o)}
      >
        {selected.length === 0 ? (
          <span className="text-gray-500 text-sm truncate">{placeholder}</span>
        ) : (
          <>
            {selected.slice(0, 2).map((o) => (
              <span
                key={o._id}
                className="inline-flex items-center gap-1 bg-blue-600/30 text-blue-200 text-xs font-medium px-2 py-0.5 rounded-full shrink-0"
              >
                <span className="max-w-[90px] truncate">{o.name}</span>
                <button
                  type="button"
                  className="text-blue-300 hover:text-white leading-none"
                  onClick={(e) => {
                    e.stopPropagation();
                    toggle(o._id);
                  }}
                >
                  &times;
                </button>
              </span>
            ))}
            {selected.length > 2 && (
              <span className="text-xs text-blue-300 font-medium shrink-0">
                +{selected.length - 2}
              </span>
            )}
          </>
        )}
        <svg
          className={`w-3.5 h-3.5 ml-auto text-gray-500 shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
          fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </div>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => { setOpen(false); setQuery(""); }} />
          <div className="absolute z-50 left-0 right-0 mt-1 rounded-lg border border-gray-700 bg-gray-900 shadow-xl overflow-hidden">
            <div className="p-2 border-b border-gray-700/60 space-y-2">
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search..."
                className="w-full bg-gray-800 text-sm text-gray-200 rounded px-2 py-1.5 outline-none border border-gray-700 focus:border-blue-500"
                autoFocus
              />
              <div className="flex items-center justify-between text-xs px-0.5">
                <span className="text-gray-400">
                  {selectedIds.length} of {options.length} selected
                </span>
                <div className="space-x-3">
                  <button
                    type="button"
                    className="text-blue-400 hover:text-blue-300"
                    onClick={() => emit(options.map((o) => o._id))}
                  >
                    Select all
                  </button>
                  <button
                    type="button"
                    className="text-gray-400 hover:text-gray-300"
                    onClick={() => emit([])}
                  >
                    Clear
                  </button>
                </div>
              </div>
            </div>
            <div className="max-h-44 overflow-y-auto py-1">
              {filtered.length === 0 && (
                <div className="px-3 py-2 text-xs text-gray-500">No matches</div>
              )}
              {filtered.map((o) => {
                const checked = selectedIds.includes(o._id);
                return (
                  <label
                    key={o._id}
                    className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer text-sm ${
                      checked ? "bg-blue-600/20 text-white" : "text-gray-300 hover:bg-white/5"
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggle(o._id)}
                      className="accent-blue-500"
                    />
                    {o.name}
                  </label>
                );
              })}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

export default function SetupScreen({ onPaired }: Props) {
  const [companyCode, setCompanyCode] = useState("");
  const [branchId, setBranchId] = useState("");
  const [shops, setShops] = useState<Shop[]>([]);
  const [shopsLoading, setShopsLoading] = useState(false);
  const [connectedAgents, setConnectedAgents] = useState<ConnectedAgent[]>([]);
  const [connectedAgentsLoading, setConnectedAgentsLoading] = useState(false);
  const [selectedAgent, setSelectedAgent] = useState<ConnectedAgent | null>(null);
  const [showAgentsList, setShowAgentsList] = useState(false);
  const [showAddNew, setShowAddNew] = useState(false);
  const [zones, setZones] = useState<string[]>([]);
  const [deviceName, setDeviceName] = useState("");
  const [deviceNameTouched, setDeviceNameTouched] = useState(false);

  // e.g. ["Bar Area"] -> "Bar Area Printer"
  //      ["Bar Area","Cubes"] -> "Bar Area & Cubes Printer"
  //      ["A","B","C"] -> "A +2 more Printer"
  const suggestDeviceName = (names: string[]): string => {
    if (names.length === 0) return "";
    if (names.length === 1) return `${names[0]} Printer`;
    if (names.length === 2) return `${names[0]} & ${names[1]} Printer`;
    return `${names[0]} +${names.length - 1} more Printer`;
  };
  const [printerType, setPrinterType] = useState<PrinterType>("serial");
  const [printerPort, setPrinterPort] = useState("");
  const [printerHost, setPrinterHost] = useState("");
  const [printerNetworkPort, setPrinterNetworkPort] = useState("9100");
  const [networkPrinters, setNetworkPrinters] = useState<string[]>([]);
  const [scanning, setScanning] = useState(false);
  const [availablePorts, setAvailablePorts] = useState<PortInfo[]>([]);
  const [windowsPrinters, setWindowsPrinters] = useState<string[]>([]);
  const [loadingWindowsPrinters, setLoadingWindowsPrinters] = useState(false);
  const [loading, setLoading] = useState(false);
  const [testPrinting, setTestPrinting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [portsLoading, setPortsLoading] = useState(false);
  const [detectingPrinters, setDetectingPrinters] = useState(false);
  const [osInfo, setOsInfo] = useState<OsInfo | null>(null);
  const [routingMode, setRoutingMode] = useState<"category" | "location">("category");
  const [routingOptions, setRoutingOptions] = useState<Array<{ _id: string; name: string }>>([]);
  const [routingOptionsLoading, setRoutingOptionsLoading] = useState(false);
  const [routingIds, setRoutingIds] = useState<string[]>([]);

  const loadPorts = async () => {
    setPortsLoading(true);
    try {
      const ports = await invoke<PortInfo[]>("detect_thermal_printers");
      setAvailablePorts(ports);
      if (ports.length > 0 && !printerPort) {
        setPrinterPort(ports[0].name);
      }
    } catch (e) {
      setAvailablePorts([]);
    } finally {
      setPortsLoading(false);
    }
  };

  const detectPrinters = async () => {
    setDetectingPrinters(true);
    try {
      const detected = await invoke<PortInfo[]>("detect_thermal_printers");
      setAvailablePorts(detected);
      if (detected.length > 0) {
        setSuccess(`Detected ${detected.length} thermal printer(s) - select one below`);
      } else {
        setError("No thermal printers detected on available ports");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to detect printers");
    } finally {
      setDetectingPrinters(false);
    }
  };

  const loadWindowsPrinters = async () => {
    setLoadingWindowsPrinters(true);
    try {
      const printers = await invoke<string[]>("list_windows_printers");
      setWindowsPrinters(printers);
      if (printers.length > 0 && !printerPort) {
        setPrinterPort(printers[0]);
      }
      if (printers.length === 0) {
        setError("No Windows printers found. Make sure your printer driver is installed.");
      }
    } catch (e) {
      setWindowsPrinters([]);
      setError(e instanceof Error ? e.message : "Failed to load Windows printers");
    } finally {
      setLoadingWindowsPrinters(false);
    }
  };

  const fetchShops = async (code: string) => {
    if (!code.trim()) {
      setShops([]);
      setBranchId("");
      setConnectedAgents([]);
      return;
    }
    setShopsLoading(true);
    setError(null);
    try {
      const shopsJson = await invoke<string>("fetch_shops", { companyCode: code.trim() });
      const shopsData = JSON.parse(shopsJson) as Shop[];
      setShops(shopsData);
      setBranchId(shopsData[0]?._id ?? "");
      setConnectedAgents([]);
      if (shopsData.length === 0) {
        setError("No shops found for this company code");
      }
    } catch (e) {
      setShops([]);
      setBranchId("");
      setConnectedAgents([]);
      const msg = e instanceof Error ? e.message : "Failed to fetch shops" + JSON.stringify(e);
      if (!msg.includes("404") && !msg.includes("Tenant not found")) {
        setError(msg);
      }
    } finally {
      setShopsLoading(false);
    }
  };

  const fetchConnectedAgents = async (code: string, shopId: string) => {
    setConnectedAgentsLoading(true);
    setError(null);
    try {
      const agentsJson = await invoke<string>("fetch_connected_agents", { companyCode: code, shopId });
      const agentsData = JSON.parse(agentsJson) as ConnectedAgentsResponse;
      setConnectedAgents(agentsData.agents);
    } catch (e) {
      setConnectedAgents([]);
      setError(e instanceof Error ? e.message : "Failed to fetch connected agents" + JSON.stringify(e));
    } finally {
      setConnectedAgentsLoading(false);
    }
  };

  const handleViewAgents = async () => {
    for (const agent of connectedAgents) {
      try {
        await invoke("auto_add_connected_agent", {
          agentId: agent.agent_id,
          companyCode: companyCode,
          companyId: agent.company_id,
          shopId: agent.shop_id,
          zone: agent.zone,
          deviceName: agent.device_name,
        });
      } catch (e) {
        console.error(`Failed to auto-add agent ${agent.device_name}:`, e);
      }
    }
    onPaired(branchId);
  };

  const scanNetworkPrinters = async () => {
    setScanning(true);
    setNetworkPrinters([]);
    setError(null);
    try {
      const found = await invoke<string[]>("discover_network_printers");
      setNetworkPrinters(found);
      if (found.length > 0 && !printerHost) {
        const first = found[0].split(" ")[0];
        setPrinterHost(first);
      }
      if (found.length === 0) setError("No printers found on port 9100. Enter the IP manually.");
    } catch (e) {
      setError("Network scan failed: " + String(e));
    } finally {
      setScanning(false);
    }
  };

  useEffect(() => {
    loadPorts();
    invoke<OsInfo>("get_os_version")
      .then((info) => {
        setOsInfo(info);
        if (info?.hostname) setDeviceName(`${info.hostname} Printer`);
      })
      .catch(() => setOsInfo(null));
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => {
      fetchShops(companyCode);
    }, 1500);
    return () => clearTimeout(timer);
  }, [companyCode]);

  useEffect(() => {
    if (branchId && companyCode.trim()) {
      fetchConnectedAgents(companyCode.trim(), branchId);
    } else {
      setConnectedAgents([]);
    }
  }, [branchId]);

  // Load the selectable routing options (main categories or locations)
  // whenever the branch or routing mode changes.
  useEffect(() => {
    setRoutingOptions([]);
    setRoutingIds([]);
    setZones([]);
    if (!branchId || !companyCode.trim()) return;
    const cmd = routingMode === "location" ? "fetch_locations" : "fetch_categories";
    setRoutingOptionsLoading(true);
    invoke<string>(cmd, { shopId: branchId, companyCode: companyCode.trim() })
      .then((json) => {
        const data = JSON.parse(json);
        const list: any[] = Array.isArray(data)
          ? data
          : data?.categories ?? data?.locations ?? data?.items ?? data?.data ?? [];
        setRoutingOptions(
          list
            .map((o: any) => ({
              _id: String(o._id ?? o.id ?? o.name),
              name: String(o.name ?? o.label ?? o._id),
            }))
            .filter((o) => o.name)
        );
      })
      .catch(() => setRoutingOptions([]))
      .finally(() => setRoutingOptionsLoading(false));
  }, [branchId, routingMode, companyCode]);

  const printerReady =
    printerType === "network"
      ? printerHost.trim().length > 0
      : printerPort.length > 0;

  const handleTestPrint = async () => {
    if (!printerReady) {
      setError(
        printerType === "network"
          ? "Enter a printer IP address first."
          : "Please select a printer port first."
      );
      return;
    }
    setTestPrinting(true);
    setError(null);
    setSuccess(null);
    try {
      await invoke("test_print", {
        printerType,
        printerPort: printerType !== "network" ? printerPort : null,
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
    if (!companyCode.trim() || !branchId.trim() || zones.length === 0 || !deviceName.trim()) {
      setError("All fields are required — select at least one " +
        (routingMode === "location" ? "location" : "category") + ".");
      return;
    }
    if (!printerReady) {
      setError(printerType === "network" ? "Enter a printer IP address." : "Select a printer port.");
      return;
    }
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      await invoke("pair_device", {
        companyCode: companyCode.trim(),
        branchId: branchId.trim(),
        zone: zones[0],
        deviceName: deviceName.trim(),
        printerPort: printerType !== "network" ? printerPort : "",
        printerType,
        printerHost: printerType === "network" ? printerHost.trim() : null,
        printerNetworkPort: printerType === "network" ? parseInt(printerNetworkPort) || 9100 : null,
        routingMode,
        mainCategoryId: routingMode === "category" ? routingIds[0] || null : null,
        location: routingMode === "location" ? routingIds[0] || null : null,
        zones,
        mainCategoryIds: routingMode === "category" ? routingIds : null,
        locations: routingMode === "location" ? routingIds : null,
      });
      setSuccess("Device paired successfully!");
      setTimeout(onPaired, 800);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleConnectToExisting = async () => {
    if (!selectedAgent || !companyCode.trim()) {
      setError("Please select an agent.");
      return;
    }
    if (!printerReady) {
      setError(printerType === "network" ? "Enter a printer IP address." : "Select a printer port.");
      return;
    }
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      await invoke("connect_to_existing_agent", {
        companyCode: companyCode.trim(),
        agentId: selectedAgent.agent_id,
        printerPort: printerType !== "network" ? printerPort : "",
        printerType,
        printerHost: printerType === "network" ? printerHost.trim() : null,
        printerNetworkPort: printerType === "network" ? parseInt(printerNetworkPort) || 9100 : null,
      });
      setSuccess("Connected to existing agent successfully!");
      setTimeout(onPaired, 800);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  // Reusable printer type toggle — used in both existing agent and add new forms
  const PrinterTypeToggle = () => (
    <Field label="Printer Connection" required>
      <div className="flex rounded-lg border border-gray-700 overflow-hidden">
        {(["serial", "network", "windows"] as PrinterType[]).map((t) => (
          <button
            key={t}
            onClick={() => {
              setPrinterType(t);
              setPrinterPort("");
              if (t === "windows") loadWindowsPrinters();
            }}
            className={`flex-1 py-2 text-sm font-medium transition-colors ${
              printerType === t
                ? "bg-green-600 text-white"
                : "bg-gray-800 text-gray-400 hover:text-gray-200"
            }`}
          >
            {t === "serial" ? "USB / Serial" : t === "network" ? "Network (TCP/IP)" : "Windows Printer"}
          </button>
        ))}
      </div>
    </Field>
  );

  // Reusable serial port section
  const SerialPortSection = () => (
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
              <option key={p.name} value={p.name}>{p.name}</option>
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
        <button
          onClick={detectPrinters}
          disabled={detectingPrinters}
          className="px-3 py-2 bg-blue-600 hover:bg-blue-700 border border-blue-500 rounded-lg text-white text-sm transition-colors disabled:opacity-50"
          title="Detect thermal printers"
        >
          {detectingPrinters ? (
            <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          ) : (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z" />
            </svg>
          )}
        </button>
      </div>
      <input
        type="text"
        value={printerPort}
        onChange={(e) => setPrinterPort(e.target.value)}
        placeholder="Or type manually, e.g. COM3 or /dev/usb/lp0"
        className="input-field mt-2"
      />
      {availablePorts.length > 0 && (
        <div className="mt-3">
          <label className="block text-xs text-gray-400 mb-2">Detected Printers</label>

          {availablePorts.filter(p => p.port_type.type_name === 'USB').length > 0 && (
            <div className="mb-3">
              <label className="block text-xs text-blue-400 mb-1">USB Ports</label>
              <div className="space-y-1">
                {availablePorts.filter(p => p.port_type.type_name === 'USB').map((port) => (
                  <div key={port.name} className="flex gap-2">
                    <button
                      type="button"
                      onClick={() => setPrinterPort(port.name)}
                      className={`flex-1 px-3 py-2 text-left text-sm rounded-lg border transition-colors ${
                        printerPort === port.name
                          ? "bg-blue-600/20 border-blue-500 text-blue-400"
                          : "bg-gray-800 border-gray-700 text-gray-300 hover:bg-gray-700"
                      }`}
                    >
                      <div className="font-medium">{port.name}</div>
                      {port.port_type.manufacturer && (
                        <div className="text-xs text-gray-500">
                          {port.port_type.manufacturer} - {port.port_type.product || ''}
                        </div>
                      )}
                    </button>
                    <button
                      type="button"
                      onClick={async () => {
                        try {
                          await invoke("test_print", { printerType: "serial", printerPort: port.name });
                          setSuccess(`Test print sent to ${port.name}`);
                        } catch (e: unknown) {
                          setError(e instanceof Error ? e.message : String(e));
                        }
                      }}
                      className="px-3 py-2 text-sm bg-gray-800 border border-gray-700 rounded-lg text-gray-300 hover:bg-gray-700 transition-colors"
                    >
                      Test
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}

          {availablePorts.filter(p => p.port_type.type_name === 'COM/Serial').length > 0 && (
            <div>
              <label className="block text-xs text-green-400 mb-1">COM / Serial Ports</label>
              <div className="space-y-1">
                {availablePorts.filter(p => p.port_type.type_name === 'COM/Serial').map((port) => (
                  <div key={port.name} className="flex gap-2">
                    <button
                      type="button"
                      onClick={() => setPrinterPort(port.name)}
                      className={`flex-1 px-3 py-2 text-left text-sm rounded-lg border transition-colors ${
                        printerPort === port.name
                          ? "bg-blue-600/20 border-blue-500 text-blue-400"
                          : "bg-gray-800 border-gray-700 text-gray-300 hover:bg-gray-700"
                      }`}
                    >
                      {port.name}
                    </button>
                    <button
                      type="button"
                      onClick={async () => {
                        try {
                          await invoke("test_print", { printerType: "serial", printerPort: port.name });
                          setSuccess(`Test print sent to ${port.name}`);
                        } catch (e: unknown) {
                          setError(e instanceof Error ? e.message : String(e));
                        }
                      }}
                      className="px-3 py-2 text-sm bg-gray-800 border border-gray-700 rounded-lg text-gray-300 hover:bg-gray-700 transition-colors"
                    >
                      Test
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </Field>
  );

  // Reusable network section
  const NetworkSection = () => (
    <div className="space-y-3">
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
  );

  // Reusable Windows printer section
  const WindowsPrinterSection = () => (
    <Field label="Windows Printer" required>
      <button
        type="button"
        onClick={loadWindowsPrinters}
        disabled={loadingWindowsPrinters}
        className="w-full py-2.5 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 text-sm font-medium transition-colors disabled:opacity-50 flex items-center justify-center gap-2 mb-2"
      >
        {loadingWindowsPrinters ? (
          <>
            <div className="w-4 h-4 border border-green-400 border-t-transparent rounded-full animate-spin" />
            Loading printers...
          </>
        ) : (
          <>
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
            Refresh Windows Printers
          </>
        )}
      </button>

      {windowsPrinters.length > 0 ? (
        <div className="space-y-1">
          {windowsPrinters.map((name) => (
            <div key={name} className="flex gap-2">
              <button
                type="button"
                onClick={() => setPrinterPort(name)}
                className={`flex-1 px-3 py-2 text-left text-sm rounded-lg border transition-colors ${
                  printerPort === name
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
                    await invoke("test_print", { printerType: "windows", printerPort: name });
                    setSuccess(`Test print sent to ${name}`);
                  } catch (e: unknown) {
                    setError(e instanceof Error ? e.message : String(e));
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
        <p className="text-sm text-gray-500 text-center py-3">
          Click "Refresh" to load installed Windows printers
        </p>
      )}
    </Field>
  );

  const PrinterSection = () => {
    if (printerType === "serial") return <SerialPortSection />;
    if (printerType === "network") return <NetworkSection />;
    return <WindowsPrinterSection />;
  };

  const TestPrintButton = () => (
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
  );

  return (
    <div className="min-h-screen bg-gray-950 flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        {/* Header */}
        <div className="text-center mb-8">
          <div className="flex flex-col items-center gap-1.5 mb-5">
            <img src="/logo.png" alt="Base" className="h-16 w-auto" />
            <span className="text-green-500/80 text-[11px] font-medium uppercase tracking-[0.25em]">
              Achieve More
            </span>
          </div>
          <h1 className="text-2xl font-semibold text-white tracking-tight">PrintAgent Setup</h1>
          <p className="text-gray-400 text-sm mt-1.5">Configure this device to connect to your print network</p>
          {osInfo && (
            <div className="mt-3">
              <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-medium border ${
                osInfo.supportsDesktopApp
                  ? "bg-gray-800/60 border-gray-700/60 text-gray-400"
                  : "bg-amber-500/10 border-amber-500/30 text-amber-400"
              }`}>
                <span className={`w-1.5 h-1.5 rounded-full ${osInfo.supportsDesktopApp ? "bg-green-500" : "bg-amber-400"}`} />
                {osInfo.name} ({osInfo.major}.{osInfo.minor}.{osInfo.build})
                {osInfo.supportsDesktopApp ? "" : " — use web version + helper"}
              </span>
            </div>
          )}
        </div>

        {/* Form Card */}
        <div className="bg-gray-900 border border-gray-800 rounded-2xl p-6 space-y-4">
          <Field label="Company Code" required>
            <input
              type="text"
              value={companyCode}
              onChange={(e) => setCompanyCode(e.target.value)}
              onKeyDown={(e) => { if (e.ctrlKey && e.key === 'p') e.preventDefault(); }}
              placeholder="e.g. ACME-2024"
              className="input-field"
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              spellCheck="false"
            />
          </Field>

          <Field label="Branch" required>
            {shopsLoading ? (
              <div className="input-field flex items-center justify-center text-gray-500 text-sm">
                <div className="w-4 h-4 border border-gray-500 border-t-transparent rounded-full animate-spin mr-2" />
                Loading shops...
              </div>
            ) : shops.length > 0 ? (
              <select
                value={branchId}
                onChange={(e) => { setBranchId(e.target.value); setSelectedAgent(null); }}
                className="input-field"
              >
                {shops.map((shop) => (
                  <option key={shop._id} value={shop._id}>
                    {shop.name} {shop.shop_code ? `(${shop.shop_code})` : ""}
                  </option>
                ))}
              </select>
            ) : companyCode.trim() ? (
              <div className="input-field text-gray-500 text-sm">No shops found for this company code</div>
            ) : (
              <div className="input-field text-gray-600 text-sm">Enter company code above to load shops</div>
            )}
          </Field>

          {/* Existing agents loading */}
          {!showAddNew && connectedAgentsLoading && (
            <div className="flex items-center justify-center py-8">
              <div className="w-6 h-6 border-2 border-green-500 border-t-transparent rounded-full animate-spin mr-2" />
              <span className="text-gray-400 text-sm">Checking for existing agents...</span>
            </div>
          )}

          {/* Existing agents list */}
          {!showAddNew && !connectedAgentsLoading && connectedAgents.length > 0 && (
            <>
              <div className="border-t border-gray-800 pt-4">
                <div className="flex items-center justify-between mb-3">
                  <h3 className="text-sm font-medium text-white">
                    Found {connectedAgents.length} Existing Agent{connectedAgents.length > 1 ? 's' : ''}
                  </h3>
                  <button onClick={() => setShowAddNew(true)} className="text-xs text-green-400 hover:text-green-300">
                    + Add New Agent
                  </button>
                </div>
                <div className="space-y-2 max-h-48 overflow-y-auto">
                  {connectedAgents.map((agent) => (
                    <button
                      key={agent.agent_id}
                      onClick={() => setSelectedAgent(agent)}
                      className={`w-full flex items-center justify-between px-3 py-2.5 rounded-lg border text-sm transition-colors ${
                        selectedAgent?.agent_id === agent.agent_id
                          ? "bg-green-500/10 border-green-500/40 text-green-400"
                          : "bg-gray-800 border-gray-700 text-gray-300 hover:border-gray-600"
                      }`}
                    >
                      <div className="flex items-center gap-2">
                        <div className={`w-2 h-2 rounded-full ${agent.connected ? "bg-green-500" : "bg-gray-500"}`} />
                        <span className="font-medium">{agent.device_name}</span>
                        <span className="text-gray-500">({agent.zone})</span>
                      </div>
                      {agent.connected && <span className="text-xs text-green-400">Connected</span>}
                    </button>
                  ))}
                </div>
              </div>

              {connectedAgents.length > 0 && !showAgentsList && (
                <div className="border-t border-gray-800 pt-4">
                  <h3 className="text-sm font-semibold text-gray-300 mb-3">
                    Found {connectedAgents.length} Existing Agents
                  </h3>
                  <button
                    onClick={handleViewAgents}
                    className="w-full py-3 bg-blue-600 hover:bg-blue-500 rounded-xl text-white font-semibold transition-colors"
                  >
                    View Print Agents
                  </button>
                </div>
              )}

              {showAgentsList && connectedAgents.length > 0 && (
                <div className="border-t border-gray-800 pt-4">
                  <div className="flex items-center justify-between mb-3">
                    <h3 className="text-sm font-semibold text-gray-300">Print Agents for this Branch</h3>
                    <button onClick={() => setShowAgentsList(false)} className="text-sm text-gray-400 hover:text-gray-300">
                      ← Back
                    </button>
                  </div>
                  <div className="space-y-2">
                    {connectedAgents.map((agent) => (
                      <button
                        key={agent.agent_id}
                        onClick={() => setSelectedAgent(agent)}
                        className={`w-full flex items-center justify-between px-4 py-3 rounded-lg border text-left transition-colors ${
                          selectedAgent?.agent_id === agent.agent_id
                            ? "bg-green-500/10 border-green-500/40 text-green-400"
                            : "bg-gray-800 border-gray-700 text-gray-300 hover:border-gray-600"
                        }`}
                      >
                        <div>
                          <div className="font-medium">{agent.device_name}</div>
                          <span className="text-sm text-gray-500">({agent.zone})</span>
                        </div>
                        {agent.connected && <span className="text-xs text-green-400">Connected</span>}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {/* Printer config for selected disconnected agent */}
              {selectedAgent && !selectedAgent.connected && (
                <div className="border-t border-gray-800 pt-4 space-y-4">
                  <div className="flex items-center gap-2 text-sm text-gray-400">
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                    <span>Configure printer for <strong className="text-white">{selectedAgent.device_name}</strong></span>
                  </div>

                  <PrinterTypeToggle />
                  <PrinterSection />
                  <TestPrintButton />

                  <button
                    onClick={handleConnectToExisting}
                    disabled={loading}
                    className="w-full py-3 bg-green-600 hover:bg-green-500 disabled:bg-green-900 disabled:text-green-700 rounded-xl text-white font-semibold transition-colors flex items-center justify-center gap-2"
                  >
                    {loading ? (
                      <>
                        <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
                        Connecting...
                      </>
                    ) : "Connect to Agent"}
                  </button>
                </div>
              )}

              {/* Already connected agent message */}
              {selectedAgent && selectedAgent.connected && (
                <div className="border-t border-gray-800 pt-4">
                  <div className="flex items-center gap-2 text-sm text-green-400">
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                    <span><strong className="text-white">{selectedAgent.device_name}</strong> is already connected and ready to print</span>
                  </div>
                </div>
              )}
            </>
          )}

          {/* No agents — show Add New button */}
          {!showAddNew && !connectedAgentsLoading && connectedAgents.length === 0 && branchId && (
            <div className="border-t border-gray-800 pt-4">
              <p className="text-sm text-gray-400 mb-3">No existing agents found for this branch</p>
              <button
                onClick={() => setShowAddNew(true)}
                className="w-full py-3 bg-green-600 hover:bg-green-500 rounded-xl text-white font-semibold transition-colors"
              >
                + Add New Agent
              </button>
            </div>
          )}

          {/* Add new agent form */}
          {showAddNew && (
            <div className="border-t border-gray-800 pt-4 space-y-4">
              <button onClick={() => setShowAddNew(false)} className="text-green-400 hover:text-green-300 text-sm">
                ← Back
              </button>

              <Field label="Route Jobs By" required>
                <div className="flex rounded-lg border border-gray-700 overflow-hidden">
                  {(["category", "location"] as const).map((m) => (
                    <button
                      key={m}
                      onClick={() => setRoutingMode(m)}
                      className={`flex-1 py-2 text-sm font-medium transition-colors ${
                        routingMode === m
                          ? "bg-green-600 text-white"
                          : "bg-gray-800 text-gray-400 hover:text-gray-200"
                      }`}
                    >
                      {m === "category" ? "Main Category" : "Location"}
                    </button>
                  ))}
                </div>
              </Field>

              <div className="grid grid-cols-2 gap-3">
                <Field label={routingMode === "location" ? "Locations" : "Zones / Categories"} required>
                  {routingOptionsLoading ? (
                    <div className="input-field flex items-center text-gray-500 text-sm">
                      <div className="w-4 h-4 border border-gray-500 border-t-transparent rounded-full animate-spin mr-2" />
                      Loading...
                    </div>
                  ) : routingOptions.length > 0 ? (
                    <OptionMultiSelect
                      options={routingOptions}
                      selectedIds={routingIds}
                      placeholder={
                        routingMode === "location"
                          ? "Select locations..."
                          : "Select categories..."
                      }
                      onChange={(ids, names) => {
                        setRoutingIds(ids);
                        setZones(names);
                        if (!deviceNameTouched) setDeviceName(suggestDeviceName(names));
                      }}
                    />
                  ) : (
                    <input
                      type="text"
                      value={zones.join(", ")}
                      onChange={(e) => {
                        const names = e.target.value.split(",").map((s) => s.trim()).filter(Boolean);
                        setZones(names);
                        setRoutingIds(names);
                      }}
                      placeholder={
                        routingMode === "location"
                          ? "e.g. Ground Floor, Patio"
                          : "e.g. Kitchen, Bar"
                      }
                      className="input-field"
                    />
                  )}
                </Field>
                <Field label="Device Name" required>
                  <input
                    type="text"
                    value={deviceName}
                    onChange={(e) => { setDeviceNameTouched(true); setDeviceName(e.target.value); }}
                    placeholder="e.g. Kitchen Printer"
                    className="input-field"
                  />
                </Field>
              </div>

              <PrinterTypeToggle />
              <PrinterSection />
              <TestPrintButton />

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
                ) : "Pair Device"}
              </button>
            </div>
          )}

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
        </div>

        <div className="text-center mt-6 space-y-0.5">
          <p className="text-gray-600 text-[11px]">
            PrintAgent v0.1.0 — Silent Print Routing
          </p>
          <p className="text-gray-700 text-[11px]">
            A product of Basepoint Cloud Inc
          </p>
        </div>
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