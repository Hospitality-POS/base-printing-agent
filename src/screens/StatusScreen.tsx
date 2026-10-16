import { useCallback, useEffect, useState } from "react";
import { invoke } from "../lib/invoke";
import { useAgentStatus } from "../hooks/useAgentStatus";
import PrinterPicker, {
  PrinterType,
  PrinterValue,
  describePrinter,
  printerArgs,
  printerIsReady,
} from "../components/PrinterPicker";
import { Alert, Badge, Button, Field, Icon, Modal, Spinner, errorMessage, useToast } from "../components/ui";

interface LocalAgent {
  agentId: string;
  zone: string;
  zones?: string[];
  deviceName: string;
  branchId: string;
  companyCode: string;
  printerType: string;
  printerPort?: string;
  printerHost?: string;
  printerNetworkPort?: number;
  routingMode?: string;
  wsUrl?: string;
  deviceToken?: string;
}

interface Props {
  onAddPrinter: () => void;
  /** Called after the last printer is removed. */
  onEmpty: () => void;
}

const toPrinterValue = (a: LocalAgent): PrinterValue => ({
  type: (["serial", "network", "windows"].includes(a.printerType) ? a.printerType : "windows") as PrinterType,
  port: a.printerPort ?? "",
  host: a.printerHost ?? "",
  networkPort: String(a.printerNetworkPort ?? 9100),
});

const zonesOf = (a: LocalAgent) => (a.zones?.length ? a.zones : a.zone ? [a.zone] : []);

export default function StatusScreen({ onAddPrinter, onEmpty }: Props) {
  const toast = useToast();
  const { statuses } = useAgentStatus();
  const [agents, setAgents] = useState<LocalAgent[] | null>(null);
  const [testing, setTesting] = useState<string | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);

  // Edit modal
  const [editing, setEditing] = useState<LocalAgent | null>(null);
  const [editName, setEditName] = useState("");
  const [editZone, setEditZone] = useState("");
  const [editPrinter, setEditPrinter] = useState<PrinterValue | null>(null);
  const [saving, setSaving] = useState(false);
  const [editError, setEditError] = useState<string | null>(null);

  // Remove modals
  const [removing, setRemoving] = useState<LocalAgent | null>(null);
  const [removeAll, setRemoveAll] = useState(false);
  const [removeBusy, setRemoveBusy] = useState(false);

  // This computer's printers come from the local config (one entry per agent).
  const loadAgents = useCallback(async () => {
    try {
      const cfg = await invoke<{ agents: LocalAgent[] }>("get_config");
      const list = cfg?.agents ?? [];
      setAgents(list);
      if (list.length === 0) onEmpty();
    } catch (e) {
      setAgents([]);
      toast("error", `Couldn't read printer settings: ${errorMessage(e)}`);
    }
  }, [onEmpty, toast]);

  useEffect(() => {
    loadAgents();
    invoke<boolean>("get_autostart").then(setAutostart).catch(() => setAutostart(null));
  }, [loadAgents]);

  const toggleAutostart = async () => {
    const next = !autostart;
    setAutostart(next);
    try {
      await invoke("set_autostart", { enabled: next });
    } catch (e) {
      setAutostart(!next);
      toast("error", `Couldn't change startup setting: ${errorMessage(e)}`);
    }
  };

  const testPrint = async (a: LocalAgent) => {
    setTesting(a.agentId);
    try {
      await invoke("test_print", printerArgs(toPrinterValue(a)));
      toast("success", `Test page sent to ${a.deviceName}.`);
    } catch (e) {
      toast("error", `Test print failed: ${errorMessage(e)}`);
    } finally {
      setTesting(null);
    }
  };

  const openEdit = (a: LocalAgent) => {
    setEditing(a);
    setEditName(a.deviceName);
    setEditZone(a.zone);
    setEditPrinter(toPrinterValue(a));
    setEditError(null);
  };

  const closeEdit = () => {
    if (saving) return;
    setEditing(null);
    setEditPrinter(null);
  };

  const saveEdit = async () => {
    if (!editing || !editPrinter) return;
    setEditError(null);
    if (!editName.trim()) return setEditError("Give this printer a name.");
    if (!printerIsReady(editPrinter)) return setEditError("Choose a printer.");
    const duplicate = (agents ?? []).find(
      (a) => a.agentId !== editing.agentId && a.deviceName.trim().toLowerCase() === editName.trim().toLowerCase()
    );
    if (duplicate) return setEditError(`Another printer on this computer is already called “${duplicate.deviceName}”.`);

    setSaving(true);
    try {
      await invoke("update_agent_printer_port", {
        agentId: editing.agentId,
        companyCode: editing.companyCode,
        ...printerArgs(editPrinter),
        deviceName: editName.trim() !== editing.deviceName ? editName.trim() : null,
        zone: editZone.trim() && editZone.trim() !== editing.zone ? editZone.trim() : null,
      });
      toast("success", `${editName.trim()} saved and synced to the server.`);
      setEditing(null);
      setEditPrinter(null);
      await loadAgents();
    } catch (e) {
      setEditError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const confirmRemove = async () => {
    if (!removing) return;
    setRemoveBusy(true);
    try {
      const warning = await invoke<string | null>("unpair_device", { agentId: removing.agentId });
      if (warning) toast("warning", warning);
      else toast("success", `${removing.deviceName} removed.`);
      setRemoving(null);
      await loadAgents();
    } catch (e) {
      toast("error", `Couldn't remove printer: ${errorMessage(e)}`);
    } finally {
      setRemoveBusy(false);
    }
  };

  const confirmRemoveAll = async () => {
    setRemoveBusy(true);
    try {
      const result = await invoke<string>("unpair_all_agents");
      toast(result.includes("could not") ? "warning" : "success", result);
      setRemoveAll(false);
      await loadAgents();
    } catch (e) {
      toast("error", `Couldn't remove printers: ${errorMessage(e)}`);
    } finally {
      setRemoveBusy(false);
    }
  };

  const connection = (a: LocalAgent) => {
    const live = statuses.find((s) => s.agentId === a.agentId)?.wsConnected ?? false;
    if (live) return { tone: "green" as const, label: "Live" };
    if (!a.wsUrl || !a.deviceToken) return { tone: "blue" as const, label: "Polling" };
    return { tone: "amber" as const, label: "Reconnecting" };
  };

  const liveCount = (agents ?? []).filter((a) => connection(a).tone === "green").length;

  return (
    <div className="h-screen flex flex-col bg-gray-950 text-gray-100">
      {/* ── Header ── */}
      <header className="flex items-center justify-between gap-4 px-8 py-5 border-b border-gray-800">
        <div className="flex items-center gap-4 min-w-0">
          <img src="/logo.png" alt="Base" className="h-8 w-auto" />
          <div className="h-8 w-px bg-gray-800" />
          <div className="min-w-0">
            <h1 className="text-base font-semibold text-white">Printers on this computer</h1>
            <p className="text-xs text-gray-400">
              {agents === null
                ? "Loading…"
                : `${agents.length} printer${agents.length === 1 ? "" : "s"} · ${liveCount} live`}
            </p>
          </div>
        </div>
        <Button variant="primary" icon="plus" onClick={onAddPrinter}>
          Add printer
        </Button>
      </header>

      {/* ── Printer grid ── */}
      <main className="flex-1 overflow-y-auto px-8 py-6">
        {agents === null ? (
          <div className="flex items-center justify-center gap-2 py-16 text-sm text-gray-500">
            <Spinner /> Loading printers…
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {agents.map((a) => {
              const conn = connection(a);
              const ready = printerIsReady(toPrinterValue(a));
              const zones = zonesOf(a);
              return (
                <article key={a.agentId} className="rounded-2xl border border-gray-800 bg-gray-900 flex flex-col">
                  <div className="p-5 flex-1">
                    <div className="flex items-start gap-3">
                      <span className="w-10 h-10 shrink-0 rounded-xl bg-gray-800 text-gray-300 flex items-center justify-center">
                        <Icon name={a.printerType === "network" ? "wifi" : a.printerType === "serial" ? "usb" : "printer"} className="w-5 h-5" />
                      </span>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 flex-wrap">
                          <h2 className="text-sm font-semibold text-white truncate">{a.deviceName || "Unnamed printer"}</h2>
                          <Badge tone={conn.tone} dot>
                            {conn.label}
                          </Badge>
                        </div>
                        <p className={`text-xs mt-0.5 truncate ${ready ? "text-gray-400" : "text-amber-300"}`}>
                          {ready ? describePrinter(toPrinterValue(a)) : "No printer selected"}
                        </p>
                      </div>
                    </div>

                    <div className="flex flex-wrap gap-1.5 mt-4">
                      <Badge tone={a.routingMode === "location" ? "purple" : "gray"}>
                        {a.routingMode === "location" ? "Location" : "Category"}
                      </Badge>
                      {zones.slice(0, 5).map((z) => (
                        <Badge key={z}>{z}</Badge>
                      ))}
                      {zones.length > 5 && <Badge>+{zones.length - 5}</Badge>}
                    </div>

                    {!ready && (
                      <div className="mt-4">
                        <Alert tone="warning">Choose which printer this should print to.</Alert>
                      </div>
                    )}
                  </div>

                  <div className="flex items-center gap-2 px-5 py-3 border-t border-gray-800">
                    <Button size="sm" icon="printer" loading={testing === a.agentId} disabled={!ready} onClick={() => testPrint(a)}>
                      Test print
                    </Button>
                    <Button size="sm" icon="edit" onClick={() => openEdit(a)}>
                      {ready ? "Edit" : "Set printer"}
                    </Button>
                    <span className="flex-1" />
                    <Button size="sm" variant="ghost" icon="trash" onClick={() => setRemoving(a)} className="hover:text-red-400">
                      Remove
                    </Button>
                  </div>
                </article>
              );
            })}

            <button
              type="button"
              onClick={onAddPrinter}
              className="rounded-2xl border border-dashed border-gray-800 hover:border-green-500/40 hover:bg-green-500/[0.03] min-h-[180px] flex flex-col items-center justify-center gap-2 text-gray-500 hover:text-green-400 transition-colors"
            >
              <Icon name="plus" className="w-6 h-6" />
              <span className="text-sm font-medium">Add another printer</span>
            </button>
          </div>
        )}
      </main>

      {/* ── Footer settings ── */}
      <footer className="flex items-center justify-between gap-4 px-8 py-3.5 border-t border-gray-800 bg-gray-900/40">
        {autostart !== null ? (
          <button type="button" onClick={toggleAutostart} className="flex items-center gap-3 text-left" role="switch" aria-checked={autostart}>
            <span className={`relative w-9 h-5 rounded-full transition-colors ${autostart ? "bg-green-600" : "bg-gray-700"}`}>
              <span className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform ${autostart ? "translate-x-4" : ""}`} />
            </span>
            <span>
              <span className="block text-sm text-gray-200">Start with Windows</span>
              <span className="block text-xs text-gray-500">Keep printing after a restart</span>
            </span>
          </button>
        ) : (
          <span className="text-[11px] text-gray-600">A product of Basepoint Cloud Inc</span>
        )}
        {(agents?.length ?? 0) > 1 && (
          <Button size="sm" variant="ghost" icon="trash" onClick={() => setRemoveAll(true)} className="hover:text-red-400">
            Remove all
          </Button>
        )}
      </footer>

      {/* ── Edit modal ── */}
      <Modal
        open={!!editing}
        size="xl"
        title={`Edit ${editing?.deviceName || "printer"}`}
        subtitle="Changes are saved to the server and take effect immediately."
        onClose={closeEdit}
        footer={
          <>
            <Button variant="ghost" onClick={closeEdit} disabled={saving}>
              Cancel
            </Button>
            <Button variant="primary" loading={saving} onClick={saveEdit}>
              Save changes
            </Button>
          </>
        }
      >
        {editing && editPrinter && (
          <div className="grid grid-cols-1 md:grid-cols-[minmax(0,0.7fr),minmax(0,1.3fr)] gap-8">
            <section className="space-y-5">
              <h3 className="text-sm font-medium text-gray-300">Details</h3>
              <Field label="Printer name" required>
                <input className="input-field" value={editName} onChange={(e) => setEditName(e.target.value)} />
              </Field>
              <Field
                label={editing.routingMode === "location" ? "Location" : "Category / zone"}
                hint={
                  zonesOf(editing).length > 1
                    ? "This printer serves several targets — changing this replaces them with one."
                    : "Must match the name the POS uses when it sends jobs."
                }
              >
                <input className="input-field" value={editZone} onChange={(e) => setEditZone(e.target.value)} />
              </Field>
              <div className="rounded-xl border border-gray-800 bg-gray-950/50 p-4 space-y-2 text-xs">
                <div className="flex justify-between gap-3">
                  <span className="text-gray-500">Agent ID</span>
                  <span className="text-gray-300 font-mono truncate">{editing.agentId}</span>
                </div>
                <div className="flex justify-between gap-3">
                  <span className="text-gray-500">Branch</span>
                  <span className="text-gray-300 font-mono truncate">{editing.branchId || "—"}</span>
                </div>
                <div className="flex justify-between gap-3">
                  <span className="text-gray-500">Company</span>
                  <span className="text-gray-300 truncate">{editing.companyCode}</span>
                </div>
              </div>
              {editError && (
                <Alert tone="error" onClose={() => setEditError(null)}>
                  {editError}
                </Alert>
              )}
            </section>
            <section>
              <h3 className="text-sm font-medium text-gray-300 mb-3">Printer</h3>
              <PrinterPicker value={editPrinter} onChange={setEditPrinter} />
            </section>
          </div>
        )}
      </Modal>

      {/* ── Remove one ── */}
      <Modal
        open={!!removing}
        size="sm"
        title={`Remove ${removing?.deviceName ?? "printer"}?`}
        onClose={() => !removeBusy && setRemoving(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setRemoving(null)} disabled={removeBusy}>
              Cancel
            </Button>
            <Button variant="danger" icon="trash" loading={removeBusy} onClick={confirmRemove}>
              Remove printer
            </Button>
          </>
        }
      >
        <p className="text-sm text-gray-400">
          This stops printing on this computer and removes the printer from the POS. Jobs for{" "}
          <span className="text-gray-200">{removing ? zonesOf(removing).join(", ") : ""}</span> won't print until another
          printer is set up for them.
        </p>
      </Modal>

      {/* ── Remove all ── */}
      <Modal
        open={removeAll}
        size="sm"
        title="Remove all printers?"
        onClose={() => !removeBusy && setRemoveAll(false)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setRemoveAll(false)} disabled={removeBusy}>
              Cancel
            </Button>
            <Button variant="danger" icon="trash" loading={removeBusy} onClick={confirmRemoveAll}>
              Remove all
            </Button>
          </>
        }
      >
        <p className="text-sm text-gray-400">
          All {agents?.length ?? 0} printers on this computer will stop printing and be removed from the POS.
        </p>
      </Modal>
    </div>
  );
}
