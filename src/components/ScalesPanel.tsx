import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "../lib/invoke";
import { Alert, Badge, Button, Field, Icon, Modal, Spinner, errorMessage, useToast } from "./ui";

export interface ScaleConfig {
  id: string;
  name: string;
  connection: "serial" | "network";
  port: string;
  host?: string | null;
  networkPort?: number | null;
  baudRate: number;
  dataBits: number;
  parity: string;
  stopBits: number;
  protocol: string;
  requestCommand?: string | null;
  tareCommand?: string | null;
  zeroCommand?: string | null;
  decimals?: number | null;
  unit: string;
}

interface WeightReading {
  weight: number | null;
  unit: string;
  stable: boolean;
  overload: boolean;
  underZero: boolean;
  raw: string;
}

interface PortInfo {
  name: string;
  port_type: { type_name: string; manufacturer?: string; product?: string };
}

const PROTOCOLS: Array<{ id: string; title: string; desc: string; serial: Partial<ScaleConfig> }> = [
  {
    id: "toledo",
    title: "Toledo",
    desc: "Mettler Toledo 8217, and Dibal / ACLAS / CAS scales in Toledo mode. Usually 9600 7-E-1.",
    serial: { baudRate: 9600, dataBits: 7, parity: "even", stopBits: 1 },
  },
  {
    id: "nci",
    title: "NCI",
    desc: "Avery Weigh-Tronix, Brecknell, Salter (NCI / SCP-01). Usually 9600 7-E-1.",
    serial: { baudRate: 9600, dataBits: 7, parity: "even", stopBits: 1 },
  },
  {
    id: "cas",
    title: "CAS continuous",
    desc: "Scale streams readings like “ST,GS,+ 1.234kg” on its own. Usually 9600 8-N-1.",
    serial: { baudRate: 9600, dataBits: 8, parity: "none", stopBits: 1 },
  },
  {
    id: "generic",
    title: "Other / generic",
    desc: "Any scale that sends a line with the weight. Add a request command if it only answers when asked.",
    serial: { baudRate: 9600, dataBits: 8, parity: "none", stopBits: 1 },
  },
];

const supportsTare = (s: ScaleConfig) =>
  s.protocol === "toledo" || s.protocol === "nci" || !!s.tareCommand;
const supportsZero = (s: ScaleConfig) =>
  s.protocol === "toledo" || s.protocol === "nci" || !!s.zeroCommand;

const emptyScale = (): ScaleConfig => ({
  id: "",
  name: "",
  connection: "serial",
  port: "",
  host: "",
  networkPort: 4001,
  protocol: "toledo",
  unit: "kg",
  decimals: null,
  requestCommand: "",
  tareCommand: "",
  zeroCommand: "",
  ...(PROTOCOLS[0].serial as Required<Pick<ScaleConfig, "baudRate" | "dataBits" | "parity" | "stopBits">>),
});

const describeScale = (s: ScaleConfig) => {
  const where = s.connection === "network" ? `${s.host || "?"}:${s.networkPort ?? 4001}` : s.port || "No port";
  const proto = PROTOCOLS.find((p) => p.id === s.protocol)?.title ?? s.protocol;
  return `${proto} · ${where}`;
};

function formatWeight(r: WeightReading | null) {
  if (!r) return "—";
  if (r.overload) return "Overload";
  if (r.weight === null) return r.stable ? "—" : "Settling…";
  const decimals = r.unit === "g" ? 0 : 3;
  return `${r.weight.toFixed(decimals)} ${r.unit}`;
}

function WeightDisplay({ reading, error }: { reading: WeightReading | null; error?: string | null }) {
  return (
    <div className="rounded-xl bg-gray-950 border border-gray-800 px-4 py-3">
      <div className="flex items-baseline justify-between gap-3">
        <span className={`font-mono text-2xl tabular-nums ${error ? "text-gray-600" : "text-white"}`}>
          {formatWeight(reading)}
        </span>
        {reading && !error && (
          <Badge tone={reading.overload ? "red" : reading.stable ? "green" : "amber"} dot>
            {reading.overload ? "Overload" : reading.stable ? "Stable" : "Moving"}
          </Badge>
        )}
      </div>
      {error && <p className="text-xs text-red-300 mt-1.5 break-words">{error}</p>}
    </div>
  );
}

function ScaleCard({ scale, onEdit, onRemove }: { scale: ScaleConfig; onEdit: () => void; onRemove: () => void }) {
  const toast = useToast();
  const [reading, setReading] = useState<WeightReading | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [live, setLive] = useState(false);
  const inFlight = useRef(false);

  const read = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    try {
      setReading(await invoke<WeightReading>("read_scale", { scaleId: scale.id }));
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }, [scale.id]);

  useEffect(() => {
    if (!live) return;
    read();
    const t = setInterval(read, 800);
    return () => clearInterval(t);
  }, [live, read]);

  const command = async (cmd: "tare" | "zero") => {
    try {
      await invoke("scale_command", { scaleId: scale.id, command: cmd });
      toast("success", `${cmd === "tare" ? "Tare" : "Zero"} sent to ${scale.name}.`);
      if (!live) read();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  return (
    <article className="rounded-2xl border border-gray-800 bg-gray-900 flex flex-col">
      <div className="p-5 flex-1 space-y-4">
        <div className="flex items-start gap-3">
          <span className="w-10 h-10 shrink-0 rounded-xl bg-gray-800 text-gray-300 flex items-center justify-center">
            <Icon name="scale" className="w-5 h-5" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2 flex-wrap">
              <h3 className="text-sm font-semibold text-white truncate">{scale.name}</h3>
              {live && (
                <Badge tone="green" dot>
                  Live
                </Badge>
              )}
            </div>
            <p className="text-xs mt-0.5 text-gray-400 truncate">{describeScale(scale)}</p>
          </div>
        </div>
        <WeightDisplay reading={reading} error={error} />
      </div>
      <div className="flex items-center gap-2 px-5 py-3 border-t border-gray-800 flex-wrap">
        <Button size="sm" icon="refresh" loading={busy && !live} onClick={read} disabled={live}>
          Read
        </Button>
        <Button size="sm" variant={live ? "primary" : "secondary"} onClick={() => setLive((l) => !l)}>
          {live ? "Stop" : "Live"}
        </Button>
        {supportsTare(scale) && (
          <Button size="sm" variant="ghost" onClick={() => command("tare")}>
            Tare
          </Button>
        )}
        {supportsZero(scale) && (
          <Button size="sm" variant="ghost" onClick={() => command("zero")}>
            Zero
          </Button>
        )}
        <span className="flex-1" />
        <Button size="sm" variant="ghost" icon="edit" onClick={onEdit} aria-label="Edit scale" />
        <Button size="sm" variant="ghost" icon="trash" onClick={onRemove} className="hover:text-red-400" aria-label="Remove scale" />
      </div>
    </article>
  );
}

function ScaleForm({ value, onChange }: { value: ScaleConfig; onChange: (v: ScaleConfig) => void }) {
  const [ports, setPorts] = useState<PortInfo[] | null>(null);
  const [advanced, setAdvanced] = useState(false);
  const [testing, setTesting] = useState(false);
  const [reading, setReading] = useState<WeightReading | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const set = (patch: Partial<ScaleConfig>) => onChange({ ...value, ...patch });

  const loadPorts = useCallback(async () => {
    setPorts(null);
    try {
      setPorts(await invoke<PortInfo[]>("detect_thermal_printers"));
    } catch {
      setPorts([]);
    }
  }, []);

  useEffect(() => {
    loadPorts();
  }, [loadPorts]);

  const test = async () => {
    setTesting(true);
    setTestError(null);
    try {
      setReading(await invoke<WeightReading>("read_scale", { scale: value }));
    } catch (e) {
      setReading(null);
      setTestError(errorMessage(e));
    } finally {
      setTesting(false);
    }
  };

  const proto = PROTOCOLS.find((p) => p.id === value.protocol) ?? PROTOCOLS[3];
  const num = (v: string) => (v.trim() === "" ? null : Number(v));

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
      <section className="space-y-5">
        <Field label="Scale name" required>
          <input
            className="input-field"
            value={value.name}
            placeholder="e.g. Butchery scale"
            onChange={(e) => set({ name: e.target.value })}
          />
        </Field>

        <Field label="Connection">
          <div className="grid grid-cols-2 gap-2">
            {(["serial", "network"] as const).map((c) => (
              <button
                key={c}
                type="button"
                onClick={() => set({ connection: c })}
                className={`rounded-lg border px-3 py-2 text-sm flex items-center gap-2 ${
                  value.connection === c
                    ? "border-green-500/50 bg-green-500/10 text-green-300"
                    : "border-gray-800 text-gray-300 hover:border-gray-700"
                }`}
              >
                <Icon name={c === "serial" ? "usb" : "wifi"} />
                {c === "serial" ? "USB / COM" : "Network"}
              </button>
            ))}
          </div>
        </Field>

        {value.connection === "serial" ? (
          <Field
            label="Port"
            required
            hint={
              <button type="button" onClick={loadPorts} className="text-green-400 hover:underline">
                Rescan ports
              </button>
            }
          >
            {ports === null ? (
              <div className="flex items-center gap-2 text-xs text-gray-500 py-2">
                <Spinner /> Looking for ports…
              </div>
            ) : (
              <>
                <select className="input-field" value={value.port} onChange={(e) => set({ port: e.target.value })}>
                  <option value="">Choose a port…</option>
                  {ports.map((p) => (
                    <option key={p.name} value={p.name}>
                      {p.name}
                      {p.port_type.product ? ` — ${p.port_type.product}` : ""}
                    </option>
                  ))}
                  {value.port && !ports.some((p) => p.name === value.port) && <option value={value.port}>{value.port}</option>}
                </select>
                {ports.length === 0 && (
                  <input
                    className="input-field mt-2"
                    value={value.port}
                    placeholder="Type a port, e.g. COM3"
                    onChange={(e) => set({ port: e.target.value })}
                  />
                )}
              </>
            )}
          </Field>
        ) : (
          <div className="grid grid-cols-[1fr,7rem] gap-3">
            <Field label="IP address" required>
              <input className="input-field" value={value.host ?? ""} placeholder="192.168.1.50" onChange={(e) => set({ host: e.target.value })} />
            </Field>
            <Field label="Port">
              <input
                className="input-field"
                inputMode="numeric"
                value={value.networkPort ?? ""}
                onChange={(e) => set({ networkPort: num(e.target.value) })}
              />
            </Field>
          </div>
        )}

        <div className="space-y-2">
          <Button size="sm" icon="scale" loading={testing} onClick={test}>
            Test reading
          </Button>
          {(reading || testError) && <WeightDisplay reading={reading} error={testError} />}
          {reading && <p className="text-[11px] text-gray-500 font-mono break-all">Raw: {reading.raw || "—"}</p>}
        </div>
      </section>

      <section className="space-y-5">
        <Field label="Protocol" hint={proto.desc}>
          <div className="grid grid-cols-2 gap-2">
            {PROTOCOLS.map((p) => (
              <button
                key={p.id}
                type="button"
                onClick={() => set({ protocol: p.id, ...p.serial })}
                className={`rounded-lg border px-3 py-2 text-sm text-left ${
                  value.protocol === p.id
                    ? "border-green-500/50 bg-green-500/10 text-green-300"
                    : "border-gray-800 text-gray-300 hover:border-gray-700"
                }`}
              >
                {p.title}
              </button>
            ))}
          </div>
        </Field>

        {value.protocol === "generic" && (
          <Field label="Request command" hint="Leave empty if the scale streams readings. Use \r for Enter, \xNN for a byte.">
            <input
              className="input-field font-mono"
              value={value.requestCommand ?? ""}
              placeholder="W\r"
              onChange={(e) => set({ requestCommand: e.target.value })}
            />
          </Field>
        )}

        <button
          type="button"
          onClick={() => setAdvanced((a) => !a)}
          className="flex items-center gap-1.5 text-xs text-gray-400 hover:text-gray-200"
        >
          <Icon name="chevronDown" className={`w-3.5 h-3.5 transition-transform ${advanced ? "" : "-rotate-90"}`} />
          Line settings &amp; units
        </button>

        {advanced && (
          <div className="space-y-4">
            {value.connection === "serial" && (
              <div className="grid grid-cols-4 gap-2">
                <Field label="Baud">
                  <select className="input-field" value={value.baudRate} onChange={(e) => set({ baudRate: Number(e.target.value) })}>
                    {[1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200].map((b) => (
                      <option key={b} value={b}>
                        {b}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label="Data bits">
                  <select className="input-field" value={value.dataBits} onChange={(e) => set({ dataBits: Number(e.target.value) })}>
                    {[7, 8].map((b) => (
                      <option key={b} value={b}>
                        {b}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label="Parity">
                  <select className="input-field" value={value.parity} onChange={(e) => set({ parity: e.target.value })}>
                    <option value="none">None</option>
                    <option value="even">Even</option>
                    <option value="odd">Odd</option>
                  </select>
                </Field>
                <Field label="Stop bits">
                  <select className="input-field" value={value.stopBits} onChange={(e) => set({ stopBits: Number(e.target.value) })}>
                    {[1, 2].map((b) => (
                      <option key={b} value={b}>
                        {b}
                      </option>
                    ))}
                  </select>
                </Field>
              </div>
            )}
            <div className="grid grid-cols-2 gap-3">
              <Field label="Default unit" hint="Used when the scale doesn't send one.">
                <select className="input-field" value={value.unit} onChange={(e) => set({ unit: e.target.value })}>
                  {["kg", "g", "lb", "oz"].map((u) => (
                    <option key={u} value={u}>
                      {u}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label="Implied decimals" hint="If “01234” means 1.234, enter 3.">
                <input
                  className="input-field"
                  inputMode="numeric"
                  value={value.decimals ?? ""}
                  placeholder="none"
                  onChange={(e) => set({ decimals: num(e.target.value) })}
                />
              </Field>
            </div>
            {(value.protocol === "generic" || value.protocol === "cas") && (
              <div className="grid grid-cols-2 gap-3">
                <Field label="Tare command">
                  <input className="input-field font-mono" value={value.tareCommand ?? ""} onChange={(e) => set({ tareCommand: e.target.value })} />
                </Field>
                <Field label="Zero command">
                  <input className="input-field font-mono" value={value.zeroCommand ?? ""} onChange={(e) => set({ zeroCommand: e.target.value })} />
                </Field>
              </div>
            )}
          </div>
        )}
      </section>
    </div>
  );
}

/** Lists, tests and configures the weighing scales on this computer. */
export default function ScalesPanel() {
  const toast = useToast();
  const [scales, setScales] = useState<ScaleConfig[] | null>(null);
  const [editing, setEditing] = useState<ScaleConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [removing, setRemoving] = useState<ScaleConfig | null>(null);

  const load = useCallback(async () => {
    try {
      setScales(await invoke<ScaleConfig[]>("list_scales"));
    } catch (e) {
      setScales([]);
      toast("error", `Couldn't read scales: ${errorMessage(e)}`);
    }
  }, [toast]);

  useEffect(() => {
    load();
  }, [load]);

  const save = async () => {
    if (!editing) return;
    setFormError(null);
    if (!editing.name.trim()) return setFormError("Give the scale a name.");
    if (editing.connection === "serial" && !editing.port.trim()) return setFormError("Choose the scale's port.");
    if (editing.connection === "network" && !editing.host?.trim()) return setFormError("Enter the scale's IP address.");
    setSaving(true);
    try {
      await invoke("save_scale", { scale: { ...editing, name: editing.name.trim() } });
      toast("success", `${editing.name.trim()} saved.`);
      setEditing(null);
      await load();
    } catch (e) {
      setFormError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    if (!removing) return;
    try {
      await invoke("remove_scale", { scaleId: removing.id });
      toast("success", `${removing.name} removed.`);
      setRemoving(null);
      await load();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  return (
    <section className="mt-10">
      <div className="flex items-center justify-between gap-4 mb-4">
        <div>
          <h2 className="text-sm font-semibold text-white">Weighing scales</h2>
          <p className="text-xs text-gray-500">The POS reads the weight from these through PrintAgent.</p>
        </div>
        <Button size="sm" icon="plus" onClick={() => (setFormError(null), setEditing(emptyScale()))}>
          Add scale
        </Button>
      </div>

      {scales === null ? (
        <div className="flex items-center gap-2 py-6 text-sm text-gray-500">
          <Spinner /> Loading scales…
        </div>
      ) : scales.length === 0 ? (
        <button
          type="button"
          onClick={() => (setFormError(null), setEditing(emptyScale()))}
          className="w-full rounded-2xl border border-dashed border-gray-800 hover:border-green-500/40 hover:bg-green-500/[0.03] py-8 flex flex-col items-center justify-center gap-2 text-gray-500 hover:text-green-400 transition-colors"
        >
          <Icon name="scale" className="w-6 h-6" />
          <span className="text-sm font-medium">Connect a weighing scale</span>
        </button>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {scales.map((s) => (
            <ScaleCard key={s.id} scale={s} onEdit={() => (setFormError(null), setEditing({ ...s }))} onRemove={() => setRemoving(s)} />
          ))}
        </div>
      )}

      <Modal
        open={!!editing}
        size="xl"
        title={editing?.id ? `Edit ${editing.name || "scale"}` : "Add a weighing scale"}
        subtitle="Pick the port and protocol, then use Test reading with something on the scale."
        onClose={() => !saving && setEditing(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setEditing(null)} disabled={saving}>
              Cancel
            </Button>
            <Button variant="primary" loading={saving} onClick={save}>
              Save scale
            </Button>
          </>
        }
      >
        {editing && (
          <div className="space-y-5">
            <ScaleForm value={editing} onChange={setEditing} />
            {formError && (
              <Alert tone="error" onClose={() => setFormError(null)}>
                {formError}
              </Alert>
            )}
          </div>
        )}
      </Modal>

      <Modal
        open={!!removing}
        size="sm"
        title={`Remove ${removing?.name ?? "scale"}?`}
        onClose={() => setRemoving(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setRemoving(null)}>
              Cancel
            </Button>
            <Button variant="danger" icon="trash" onClick={remove}>
              Remove scale
            </Button>
          </>
        }
      >
        <p className="text-sm text-gray-400">The POS will no longer be able to read weights from this scale.</p>
      </Modal>
    </section>
  );
}
