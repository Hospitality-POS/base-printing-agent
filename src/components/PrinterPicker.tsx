import { useEffect, useState } from "react";
import { invoke } from "../lib/invoke";
import { Button, Field, Icon, IconName, Spinner, errorMessage, useToast } from "./ui";

export type PrinterType = "serial" | "network" | "windows";

export interface PrinterValue {
  type: PrinterType;
  /** COM port / device path (serial) or printer name (windows). */
  port: string;
  host: string;
  networkPort: string;
}

export const emptyPrinter = (type: PrinterType = "windows"): PrinterValue => ({
  type,
  port: "",
  host: "",
  networkPort: "9100",
});

export const printerIsReady = (v: PrinterValue) =>
  v.type === "network" ? v.host.trim().length > 0 : v.port.trim().length > 0;

/** Arguments shared by pair / connect / update / test_print commands. */
export const printerArgs = (v: PrinterValue) => ({
  printerType: v.type,
  printerPort: v.type !== "network" ? v.port.trim() : "",
  printerHost: v.type === "network" ? v.host.trim() : null,
  printerNetworkPort: v.type === "network" ? parseInt(v.networkPort) || 9100 : null,
});

export const describePrinter = (v: { type: string; port?: string; host?: string; networkPort?: string | number }) => {
  if (v.type === "network") return v.host ? `${v.host}:${v.networkPort || 9100}` : "No IP set";
  return v.port || "Not selected";
};

interface PortInfo {
  name: string;
  port_type: { type_name: string; manufacturer?: string; product?: string };
}

interface Device {
  key: string;
  title: string;
  detail?: string;
  apply: Partial<PrinterValue>;
}

const TYPES: Array<{ type: PrinterType; icon: IconName; title: string; desc: string }> = [
  { type: "windows", icon: "windows", title: "Windows", desc: "Installed driver — easiest" },
  { type: "network", icon: "wifi", title: "Network", desc: "Printer with an IP" },
  { type: "serial", icon: "usb", title: "USB / COM", desc: "Direct port" },
];

export default function PrinterPicker({
  value,
  onChange,
  language = "escpos",
}: {
  value: PrinterValue;
  onChange: (v: PrinterValue) => void;
  /** Printer language, so the test is a receipt page or a ZPL/TSPL label. */
  language?: "escpos" | "zpl" | "tspl";
}) {
  const toast = useToast();
  const [devices, setDevices] = useState<Device[]>([]);
  const [loading, setLoading] = useState(false);
  const [testing, setTesting] = useState<string | null>(null);

  const set = (patch: Partial<PrinterValue>) => onChange({ ...value, ...patch });

  const load = async (type: PrinterType, scan = false) => {
    setLoading(true);
    try {
      if (type === "windows") {
        const names = await invoke<string[]>("list_windows_printers");
        setDevices(names.map((n) => ({ key: n, title: n, apply: { port: n } })));
      } else if (type === "serial") {
        const ports = await invoke<PortInfo[]>("detect_thermal_printers");
        setDevices(
          ports.map((p) => ({
            key: p.name,
            title: p.name,
            detail: [p.port_type.type_name, p.port_type.manufacturer, p.port_type.product]
              .filter(Boolean)
              .join(" · "),
            apply: { port: p.name },
          }))
        );
      } else if (scan) {
        const found = await invoke<string[]>("discover_network_printers");
        setDevices(
          found.map((entry) => {
            const ip = entry.split(" ")[0];
            return { key: ip, title: ip, detail: "Answers on port 9100", apply: { host: ip, networkPort: "9100" } };
          })
        );
        if (found.length === 0) toast("info", "No printers answered on port 9100. Enter the IP address manually.");
      }
    } catch (e) {
      setDevices([]);
      toast("error", errorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  // Load the device list whenever the connection type changes (network scans
  // take a few seconds, so those only run on request).
  useEffect(() => {
    setDevices([]);
    if (value.type !== "network") load(value.type);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value.type]);

  const isSelected = (d: Device) =>
    value.type === "network" ? value.host === d.apply.host : value.port === d.apply.port;

  const testPrint = async (target: PrinterValue, key: string) => {
    setTesting(key);
    try {
      await invoke("test_print", { ...printerArgs(target), printerLanguage: language });
      toast("success", `Test ${language === "escpos" ? "page" : "label"} sent to ${describePrinter(target)}`);
    } catch (e) {
      toast("error", `Test print failed: ${errorMessage(e)}`);
    } finally {
      setTesting(null);
    }
  };

  return (
    <div className="space-y-5">
      {/* Connection type */}
      <div className="grid grid-cols-3 gap-2.5">
        {TYPES.map((t) => {
          const active = value.type === t.type;
          return (
            <button
              key={t.type}
              type="button"
              onClick={() => active || onChange({ ...emptyPrinter(t.type) })}
              className={`text-left rounded-xl border p-3.5 transition-colors ${
                active
                  ? "border-green-500/60 bg-green-500/10"
                  : "border-gray-800 bg-gray-900 hover:border-gray-700 hover:bg-gray-800/60"
              }`}
            >
              <div className="flex items-center gap-2.5">
                <span
                  className={`w-8 h-8 rounded-lg flex items-center justify-center ${
                    active ? "bg-green-500/20 text-green-400" : "bg-gray-800 text-gray-400"
                  }`}
                >
                  <Icon name={t.icon} />
                </span>
                <span className={`text-sm font-medium ${active ? "text-white" : "text-gray-200"}`}>{t.title}</span>
              </div>
              <p className="text-xs text-gray-500 mt-2">{t.desc}</p>
            </button>
          );
        })}
      </div>

      {/* Device list */}
      <div className="rounded-xl border border-gray-800 overflow-hidden">
        <div className="flex items-center justify-between px-4 py-2.5 bg-gray-900 border-b border-gray-800">
          <span className="text-xs font-medium text-gray-400">
            {value.type === "windows"
              ? "Printers installed on this computer"
              : value.type === "serial"
                ? "Detected ports"
                : "Printers found on your network"}
          </span>
          <Button
            size="sm"
            variant="ghost"
            icon={value.type === "network" ? "search" : "refresh"}
            loading={loading}
            onClick={() => load(value.type, true)}
          >
            {value.type === "network" ? "Scan network" : "Refresh"}
          </Button>
        </div>

        <div className="max-h-56 overflow-y-auto divide-y divide-gray-800/70">
          {loading && devices.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-8 text-sm text-gray-500">
              <Spinner />
              {value.type === "network" ? "Scanning your network — this takes a few seconds…" : "Looking for printers…"}
            </div>
          ) : devices.length === 0 ? (
            <div className="py-8 text-center text-sm text-gray-500">
              {value.type === "network"
                ? "Click “Scan network” to find printers, or type the IP below."
                : value.type === "windows"
                  ? "No installed printers found. Install the printer driver, then refresh."
                  : "No ports detected. Plug in the printer and refresh, or type the port below."}
            </div>
          ) : (
            devices.map((d) => {
              const selected = isSelected(d);
              return (
                <div
                  key={d.key}
                  className={`flex items-center gap-3 px-4 py-2.5 cursor-pointer transition-colors ${
                    selected ? "bg-green-500/10" : "hover:bg-white/[0.03]"
                  }`}
                  onClick={() => set(d.apply)}
                >
                  <span
                    className={`w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 ${
                      selected ? "border-green-500" : "border-gray-600"
                    }`}
                  >
                    {selected && <span className="w-2 h-2 rounded-full bg-green-500" />}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className={`text-sm truncate ${selected ? "text-white font-medium" : "text-gray-200"}`}>
                      {d.title}
                    </div>
                    {d.detail && <div className="text-xs text-gray-500 truncate">{d.detail}</div>}
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    icon="printer"
                    loading={testing === d.key}
                    onClick={(e) => {
                      e.stopPropagation();
                      testPrint({ ...value, ...d.apply }, d.key);
                    }}
                  >
                    Test
                  </Button>
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Manual entry */}
      {value.type === "network" ? (
        <div className="grid grid-cols-[1fr,120px] gap-3">
          <Field label="Printer IP address" required>
            <input
              className="input-field"
              value={value.host}
              onChange={(e) => set({ host: e.target.value })}
              placeholder="e.g. 192.168.1.100"
              spellCheck={false}
            />
          </Field>
          <Field label="Port">
            <input
              className="input-field"
              type="number"
              min={1}
              max={65535}
              value={value.networkPort}
              onChange={(e) => set({ networkPort: e.target.value })}
            />
          </Field>
        </div>
      ) : (
        <Field
          label={value.type === "windows" ? "Printer name" : "Port"}
          hint={
            value.type === "windows"
              ? "Pick from the list above, or type the exact name from Settings → Printers & scanners."
              : "Pick from the list above, or type it, e.g. COM3."
          }
        >
          <input
            className="input-field"
            value={value.port}
            onChange={(e) => set({ port: e.target.value })}
            placeholder={value.type === "windows" ? "e.g. POS-80" : "e.g. COM3"}
            spellCheck={false}
          />
        </Field>
      )}

      <div className="flex items-center justify-between gap-3 rounded-xl bg-gray-950/60 border border-gray-800 px-4 py-3">
        <div className="min-w-0">
          <div className="text-xs text-gray-500">Selected printer</div>
          <div className={`text-sm truncate ${printerIsReady(value) ? "text-white" : "text-gray-500"}`}>
            {describePrinter(value)}
          </div>
        </div>
        <Button
          icon="printer"
          disabled={!printerIsReady(value)}
          loading={testing === "__selected"}
          onClick={() => testPrint(value, "__selected")}
        >
          Print test page
        </Button>
      </div>
    </div>
  );
}
