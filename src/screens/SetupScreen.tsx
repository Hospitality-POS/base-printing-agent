import { useEffect, useMemo, useState } from "react";
import { invoke } from "../lib/invoke";
import PrinterPicker, {
  PrinterValue,
  describePrinter,
  emptyPrinter,
  printerArgs,
  printerIsReady,
} from "../components/PrinterPicker";
import { Alert, Badge, Button, Field, Icon, Spinner, errorMessage, useToast } from "../components/ui";

interface Props {
  onPaired: (branchId: string) => void;
  /** Shown when this computer already has printers, to go back without changes. */
  onCancel?: () => void;
}

type ApiEnvironment = "live" | "local";
type Step = "connect" | "choose" | "printer";

const LIVE_API_URL = "https://api.hospitality.reliatech.co.ke";
const LOCAL_API_URL =
  (import.meta.env.VITE_PRINT_AGENT_LOCAL_API_URL as string | undefined) ?? "http://localhost:3002";

const normalizeApiUrl = (url: string) => url.trim().replace(/\/+$/, "");

interface Shop {
  _id: string;
  name: string;
  shop_code?: string;
}

interface ServerAgent {
  agent_id: string;
  company_id: string;
  shop_id: string;
  zone: string;
  zones?: string[];
  routing_mode?: string;
  main_category_ids?: string[];
  location_ids?: string[];
  device_name: string;
  connected: boolean;
  last_seen?: string;
  printer_port?: string;
  printer_type?: string;
  printer_host?: string;
  printer_network_port?: number;
}

interface OsInfo {
  name: string;
  major: number;
  minor: number;
  build: number;
  supportsDesktopApp: boolean;
  hostname?: string;
}

interface Option {
  _id: string;
  name: string;
}

const agentZones = (a: ServerAgent) => (a.zones?.length ? a.zones : a.zone ? [a.zone] : []);

const suggestDeviceName = (names: string[]) => {
  if (names.length === 0) return "";
  if (names.length === 1) return `${names[0]} Printer`;
  if (names.length === 2) return `${names[0]} & ${names[1]} Printer`;
  return `${names[0]} +${names.length - 1} more Printer`;
};

const STEPS: Array<{ id: Step; title: string; desc: string }> = [
  { id: "connect", title: "Your business", desc: "Company code and branch" },
  { id: "choose", title: "Choose printer", desc: "Reuse one or add new" },
  { id: "printer", title: "Connect printer", desc: "Pick the device and test" },
];

export default function SetupScreen({ onPaired, onCancel }: Props) {
  const toast = useToast();
  const [step, setStep] = useState<Step>("connect");

  // ── Step 1: business ──────────────────────────────────────────────────────
  const [apiEnvironment, setApiEnvironment] = useState<ApiEnvironment>("live");
  const [apiUrl, setApiUrl] = useState(LIVE_API_URL);
  const [apiUrlLoaded, setApiUrlLoaded] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [companyCode, setCompanyCode] = useState("");
  const [shops, setShops] = useState<Shop[]>([]);
  const [shopsLoading, setShopsLoading] = useState(false);
  const [shopsError, setShopsError] = useState<string | null>(null);
  const [branchId, setBranchId] = useState("");
  const [osInfo, setOsInfo] = useState<OsInfo | null>(null);

  // ── Step 2: choose ────────────────────────────────────────────────────────
  const [serverAgents, setServerAgents] = useState<ServerAgent[]>([]);
  const [agentsLoading, setAgentsLoading] = useState(false);
  const [localAgentIds, setLocalAgentIds] = useState<string[]>([]);
  const [mode, setMode] = useState<"new" | "existing" | null>(null);
  const [selectedAgent, setSelectedAgent] = useState<ServerAgent | null>(null);

  // ── Step 3: printer + routing ────────────────────────────────────────────
  const [routingMode, setRoutingMode] = useState<"category" | "location">("category");
  const [routingOptions, setRoutingOptions] = useState<Option[]>([]);
  const [routingLoading, setRoutingLoading] = useState(false);
  const [routingIds, setRoutingIds] = useState<string[]>([]);
  const [routingQuery, setRoutingQuery] = useState("");
  const [manualZones, setManualZones] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const [deviceNameTouched, setDeviceNameTouched] = useState(false);
  const [printer, setPrinter] = useState<PrinterValue>(emptyPrinter());
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const apiUrlIsValid = /^https?:\/\//.test(normalizeApiUrl(apiUrl));
  const code = companyCode.trim();
  const branch = shops.find((s) => s._id === branchId);

  // Initial load: OS badge, saved API URL, agents already on this computer.
  useEffect(() => {
    invoke<OsInfo>("get_os_version").then(setOsInfo).catch(() => setOsInfo(null));
    invoke<{ agents: Array<{ agentId: string; companyCode: string }> }>("get_config")
      .then((cfg) => {
        setLocalAgentIds((cfg?.agents ?? []).map((a) => a.agentId));
        const known = cfg?.agents?.[0]?.companyCode;
        if (known) setCompanyCode(known);
      })
      .catch(() => setLocalAgentIds([]));
    invoke<string>("get_api_url")
      .then((url) => {
        const normalized = normalizeApiUrl(url || LIVE_API_URL) || LIVE_API_URL;
        setApiUrl(normalized);
        setApiEnvironment(normalized === LIVE_API_URL ? "live" : "local");
        if (normalized !== LIVE_API_URL) setShowAdvanced(true);
      })
      .catch(() => setApiUrl(LIVE_API_URL))
      .finally(() => setApiUrlLoaded(true));
  }, []);

  // Persist the API URL (debounced) so every later call uses it.
  useEffect(() => {
    if (!apiUrlLoaded || !apiUrlIsValid) return;
    const t = setTimeout(() => {
      invoke("set_api_url", { url: normalizeApiUrl(apiUrl) }).catch((e) => toast("error", errorMessage(e)));
    }, 300);
    return () => clearTimeout(t);
  }, [apiUrl, apiUrlLoaded]);

  // Look up branches shortly after the company code stops changing.
  useEffect(() => {
    if (!apiUrlLoaded) return;
    setShops([]);
    setBranchId("");
    setShopsError(null);
    if (!code) return;
    const t = setTimeout(async () => {
      setShopsLoading(true);
      try {
        await invoke("set_api_url", { url: normalizeApiUrl(apiUrl) });
        const list = JSON.parse(await invoke<string>("fetch_shops", { companyCode: code })) as Shop[];
        setShops(list);
        setBranchId(list.length === 1 ? list[0]._id : "");
        if (list.length === 0) setShopsError("No branches found for this company code.");
      } catch (e) {
        const msg = errorMessage(e);
        setShopsError(
          msg.includes("404") || msg.includes("Tenant not found")
            ? "We couldn't find that company code. Check it and try again."
            : msg
        );
      } finally {
        setShopsLoading(false);
      }
    }, 700);
    return () => clearTimeout(t);
  }, [code, apiUrl, apiUrlLoaded]);

  const loadServerAgents = async () => {
    setAgentsLoading(true);
    try {
      const data = JSON.parse(
        await invoke<string>("fetch_connected_agents", { companyCode: code, shopId: branchId })
      ) as { agents: ServerAgent[] };
      // The server can briefly return the same agent twice — keep one.
      const unique = new Map<string, ServerAgent>();
      for (const a of data.agents ?? []) unique.set(a.agent_id, a);
      setServerAgents([...unique.values()]);
    } catch (e) {
      setServerAgents([]);
      toast("error", `Couldn't load printers for this branch: ${errorMessage(e)}`);
    } finally {
      setAgentsLoading(false);
    }
  };

  // Routing options for a new printer.
  useEffect(() => {
    if (step !== "printer" || mode !== "new") return;
    setRoutingOptions([]);
    setRoutingIds([]);
    setRoutingQuery("");
    const cmd = routingMode === "location" ? "fetch_locations" : "fetch_categories";
    setRoutingLoading(true);
    invoke<string>(cmd, { shopId: branchId, companyCode: code })
      .then((json) => {
        const data = JSON.parse(json);
        const list: any[] = Array.isArray(data)
          ? data
          : (data?.categories ?? data?.locations ?? data?.items ?? data?.data ?? []);
        const seen = new Set<string>();
        setRoutingOptions(
          list
            .map((o: any) => ({ _id: String(o._id ?? o.id ?? o.name), name: String(o.name ?? o.label ?? o._id) }))
            .filter((o) => o.name && !seen.has(o._id) && (seen.add(o._id), true))
        );
      })
      .catch(() => setRoutingOptions([]))
      .finally(() => setRoutingLoading(false));
  }, [step, mode, routingMode, branchId, code]);

  // Which routing targets are already served by another printer in the branch.
  const claimedBy = useMemo(() => {
    const map = new Map<string, string>();
    for (const a of serverAgents) {
      for (const z of agentZones(a)) map.set(`name:${z.toLowerCase()}`, a.device_name);
      for (const id of [...(a.main_category_ids ?? []), ...(a.location_ids ?? [])]) map.set(`id:${id}`, a.device_name);
    }
    return map;
  }, [serverAgents]);

  const optionOwner = (o: Option) => claimedBy.get(`id:${o._id}`) ?? claimedBy.get(`name:${o.name.toLowerCase()}`);

  const selectedNames = routingOptions.length
    ? routingOptions.filter((o) => routingIds.includes(o._id)).map((o) => o.name)
    : manualZones.split(",").map((s) => s.trim()).filter(Boolean);

  const toggleRouting = (id: string) => {
    const next = routingIds.includes(id) ? routingIds.filter((i) => i !== id) : [...routingIds, id];
    setRoutingIds(next);
    if (!deviceNameTouched) {
      setDeviceName(suggestDeviceName(routingOptions.filter((o) => next.includes(o._id)).map((o) => o.name)));
    }
  };

  // ── Navigation ───────────────────────────────────────────────────────────
  const goChoose = async () => {
    if (!apiUrlIsValid) return toast("error", "Enter a valid API URL starting with http:// or https://.");
    setMode(null);
    setSelectedAgent(null);
    setStep("choose");
    await loadServerAgents();
  };

  const goPrinter = (nextMode: "new" | "existing", agent?: ServerAgent) => {
    setMode(nextMode);
    setFormError(null);
    if (nextMode === "existing" && agent) {
      setSelectedAgent(agent);
      const type = (agent.printer_type as PrinterValue["type"]) || "windows";
      setPrinter({
        type: ["serial", "network", "windows"].includes(type) ? type : "windows",
        port: agent.printer_port ?? "",
        host: agent.printer_host ?? "",
        networkPort: String(agent.printer_network_port ?? 9100),
      });
    } else {
      setSelectedAgent(null);
      setPrinter(emptyPrinter());
      setDeviceName(osInfo?.hostname ? `${osInfo.hostname} Printer` : "");
      setDeviceNameTouched(false);
      setManualZones("");
    }
    setStep("printer");
  };

  // ── Submit ───────────────────────────────────────────────────────────────
  const submit = async () => {
    setFormError(null);
    if (!printerIsReady(printer)) {
      return setFormError(printer.type === "network" ? "Enter the printer's IP address." : "Choose a printer.");
    }

    if (mode === "existing" && selectedAgent) {
      setSaving(true);
      try {
        await invoke("connect_to_existing_agent", {
          companyCode: code,
          agentId: selectedAgent.agent_id,
          shopId: selectedAgent.shop_id || branchId,
          zone: selectedAgent.zone,
          zones: agentZones(selectedAgent),
          routingMode: selectedAgent.routing_mode ?? null,
          deviceName: selectedAgent.device_name,
          companyId: selectedAgent.company_id,
          ...printerArgs(printer),
        });
        toast("success", `${selectedAgent.device_name} is now printing from this computer.`);
        onPaired(branchId);
      } catch (e) {
        setFormError(errorMessage(e));
      } finally {
        setSaving(false);
      }
      return;
    }

    // New printer — guard against duplicates before calling the API.
    const zones = selectedNames;
    if (zones.length === 0) {
      return setFormError(`Select at least one ${routingMode === "location" ? "location" : "category"}.`);
    }
    const name = deviceName.trim();
    if (!name) return setFormError("Give this printer a name.");
    const nameTaken = serverAgents.find((a) => a.device_name?.trim().toLowerCase() === name.toLowerCase());
    if (nameTaken) {
      return setFormError(`A printer called “${nameTaken.device_name}” already exists in this branch. Choose another name, or go back and reuse it.`);
    }
    const clash = zones.map((z) => [z, claimedBy.get(`name:${z.toLowerCase()}`)] as const).find(([, owner]) => owner);
    if (clash) {
      return setFormError(`“${clash[0]}” is already printed by ${clash[1]}. Go back and reuse that printer, or pick different ${routingMode === "location" ? "locations" : "categories"}.`);
    }

    setSaving(true);
    try {
      const ids = routingOptions.length ? routingIds : zones;
      await invoke("pair_device", {
        companyCode: code,
        branchId,
        zone: zones[0],
        deviceName: name,
        ...printerArgs(printer),
        routingMode,
        mainCategoryId: routingMode === "category" ? ids[0] || null : null,
        location: routingMode === "location" ? ids[0] || null : null,
        zones,
        mainCategoryIds: routingMode === "category" ? ids : null,
        locations: routingMode === "location" ? ids : null,
      });
      toast("success", `${name} is paired and ready to print.`);
      onPaired(branchId);
    } catch (e) {
      setFormError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const stepIndex = STEPS.findIndex((s) => s.id === step);
  const filteredOptions = routingQuery
    ? routingOptions.filter((o) => o.name.toLowerCase().includes(routingQuery.toLowerCase()))
    : routingOptions;
  const freeOptions = routingOptions.filter((o) => !optionOwner(o));

  return (
    <div className="h-screen flex bg-gray-950 text-gray-100">
      {/* ── Sidebar ─────────────────────────────────────────────────────── */}
      <aside className="hidden md:flex w-64 shrink-0 flex-col border-r border-gray-800 bg-gray-900/40 px-6 py-7">
        <img src="/logo.png" alt="Base" className="h-10 w-auto self-start" />
        <h1 className="mt-6 text-lg font-semibold text-white">Set up a printer</h1>
        <p className="text-sm text-gray-400 mt-1">Connect this computer to your POS in three short steps.</p>

        <ol className="mt-8 space-y-1">
          {STEPS.map((s, i) => {
            const done = i < stepIndex;
            const active = i === stepIndex;
            return (
              <li key={s.id} className={`flex gap-3 rounded-lg px-2 py-2 ${active ? "bg-white/[0.04]" : ""}`}>
                <span
                  className={`mt-0.5 w-6 h-6 shrink-0 rounded-full flex items-center justify-center text-xs font-semibold ${
                    done
                      ? "bg-green-600 text-gray-950"
                      : active
                        ? "border-2 border-green-500 text-green-400"
                        : "border border-gray-700 text-gray-500"
                  }`}
                >
                  {done ? <Icon name="check" className="w-3.5 h-3.5" /> : i + 1}
                </span>
                <div>
                  <div className={`text-sm font-medium ${active ? "text-white" : done ? "text-gray-300" : "text-gray-500"}`}>
                    {s.title}
                  </div>
                  <div className="text-xs text-gray-500">{s.desc}</div>
                </div>
              </li>
            );
          })}
        </ol>

        <div className="mt-auto space-y-3">
          {osInfo && (
            <Badge tone={osInfo.supportsDesktopApp ? "gray" : "amber"} dot>
              {osInfo.name} {osInfo.supportsDesktopApp ? "" : "· use web version"}
            </Badge>
          )}
          <p className="text-[11px] text-gray-600">A product of Basepoint Cloud Inc</p>
        </div>
      </aside>

      {/* ── Content ─────────────────────────────────────────────────────── */}
      <main className="flex-1 flex flex-col min-w-0">
        <header className="flex items-center justify-between gap-4 px-8 pt-7 pb-5">
          <div>
            <p className="text-xs font-medium text-green-400 uppercase tracking-wider">
              Step {stepIndex + 1} of {STEPS.length}
            </p>
            <h2 className="text-xl font-semibold text-white mt-1">
              {step === "connect"
                ? "Which business is this printer for?"
                : step === "choose"
                  ? `Printers at ${branch?.name ?? "this branch"}`
                  : mode === "existing"
                    ? `Connect “${selectedAgent?.device_name}” to this computer`
                    : "Add a new printer"}
            </h2>
          </div>
          {onCancel && (
            <Button variant="ghost" icon="x" onClick={onCancel}>
              Cancel
            </Button>
          )}
        </header>

        <div className="flex-1 overflow-y-auto px-8 pb-6">
          {/* ── Step 1 ── */}
          {step === "connect" && (
            <div className="max-w-xl space-y-5">
              <Field label="Company code" required hint="You'll find it in the POS under Settings → Printers.">
                <div className="relative">
                  <input
                    className="input-field pr-10 text-base"
                    value={companyCode}
                    onChange={(e) => setCompanyCode(e.target.value)}
                    placeholder="e.g. ACME-2024"
                    autoFocus
                    autoComplete="off"
                    spellCheck={false}
                  />
                  {shopsLoading && (
                    <span className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400">
                      <Spinner />
                    </span>
                  )}
                </div>
              </Field>

              {shopsError && <Alert tone="error">{shopsError}</Alert>}

              {shops.length > 0 && (
                <Field label="Branch" required>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                    {shops.map((s) => {
                      const active = s._id === branchId;
                      return (
                        <button
                          key={s._id}
                          type="button"
                          onClick={() => setBranchId(s._id)}
                          className={`text-left rounded-xl border px-4 py-3 transition-colors ${
                            active ? "border-green-500/60 bg-green-500/10" : "border-gray-800 hover:border-gray-700 hover:bg-gray-900"
                          }`}
                        >
                          <div className={`text-sm font-medium ${active ? "text-white" : "text-gray-200"}`}>{s.name}</div>
                          {s.shop_code && <div className="text-xs text-gray-500 mt-0.5">{s.shop_code}</div>}
                        </button>
                      );
                    })}
                  </div>
                </Field>
              )}

              <div>
                <button
                  type="button"
                  onClick={() => setShowAdvanced((v) => !v)}
                  className="flex items-center gap-1.5 text-xs text-gray-500 hover:text-gray-300"
                >
                  <Icon name="chevronDown" className={`w-3.5 h-3.5 transition-transform ${showAdvanced ? "rotate-180" : ""}`} />
                  Advanced: server
                </button>
                {showAdvanced && (
                  <div className="mt-3 grid grid-cols-[120px,1fr] gap-2">
                    <select
                      className="input-field"
                      value={apiEnvironment}
                      onChange={(e) => {
                        const env = e.target.value as ApiEnvironment;
                        setApiEnvironment(env);
                        setApiUrl(env === "live" ? LIVE_API_URL : LOCAL_API_URL);
                      }}
                    >
                      <option value="live">Live</option>
                      <option value="local">Local</option>
                    </select>
                    <input
                      className="input-field disabled:opacity-60"
                      value={apiUrl}
                      onChange={(e) => setApiUrl(e.target.value)}
                      disabled={apiEnvironment === "live"}
                      spellCheck={false}
                    />
                  </div>
                )}
              </div>
            </div>
          )}

          {/* ── Step 2 ── */}
          {step === "choose" && (
            <div className="space-y-4">
              <button
                type="button"
                onClick={() => goPrinter("new")}
                className="w-full flex items-center gap-4 rounded-xl border border-dashed border-green-500/40 bg-green-500/[0.04] hover:bg-green-500/10 px-5 py-4 text-left transition-colors"
              >
                <span className="w-10 h-10 rounded-xl bg-green-500/15 text-green-400 flex items-center justify-center">
                  <Icon name="plus" className="w-5 h-5" />
                </span>
                <div className="flex-1">
                  <div className="text-sm font-medium text-white">Add a new printer</div>
                  <div className="text-xs text-gray-400">For a station that doesn't have a printer yet, e.g. a new kitchen or bar.</div>
                </div>
                <Icon name="arrowRight" className="w-4 h-4 text-gray-500" />
              </button>

              <div className="flex items-center justify-between pt-2">
                <h3 className="text-sm font-medium text-gray-300">
                  Existing printers {serverAgents.length > 0 && <span className="text-gray-500">({serverAgents.length})</span>}
                </h3>
                <Button size="sm" variant="ghost" icon="refresh" loading={agentsLoading} onClick={loadServerAgents}>
                  Refresh
                </Button>
              </div>

              {agentsLoading && serverAgents.length === 0 ? (
                <div className="flex items-center justify-center gap-2 py-10 text-sm text-gray-500">
                  <Spinner /> Loading printers…
                </div>
              ) : serverAgents.length === 0 ? (
                <div className="rounded-xl border border-gray-800 py-10 text-center text-sm text-gray-500">
                  This branch has no printers yet — add a new one above.
                </div>
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {serverAgents.map((a) => {
                    const here = localAgentIds.includes(a.agent_id);
                    return (
                      <button
                        key={a.agent_id}
                        type="button"
                        disabled={here}
                        onClick={() => goPrinter("existing", a)}
                        className="group text-left rounded-xl border border-gray-800 bg-gray-900 hover:border-gray-700 hover:bg-gray-800/60 disabled:opacity-60 disabled:hover:bg-gray-900 disabled:cursor-not-allowed px-4 py-3.5 transition-colors"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="min-w-0">
                            <div className="text-sm font-medium text-white truncate">{a.device_name}</div>
                            <div className="text-xs text-gray-500 mt-0.5 truncate">
                              {describePrinter({
                                type: a.printer_type ?? "",
                                port: a.printer_port,
                                host: a.printer_host,
                                networkPort: a.printer_network_port,
                              })}
                            </div>
                          </div>
                          {here ? (
                            <Badge tone="blue">On this computer</Badge>
                          ) : a.connected ? (
                            <Badge tone="green" dot>Online</Badge>
                          ) : (
                            <Badge dot>Offline</Badge>
                          )}
                        </div>
                        <div className="flex flex-wrap gap-1.5 mt-3">
                          <Badge tone={a.routing_mode === "location" ? "purple" : "gray"}>
                            {a.routing_mode === "location" ? "Location" : "Category"}
                          </Badge>
                          {agentZones(a).slice(0, 4).map((z) => (
                            <Badge key={z}>{z}</Badge>
                          ))}
                          {agentZones(a).length > 4 && <Badge>+{agentZones(a).length - 4}</Badge>}
                        </div>
                      </button>
                    );
                  })}
                </div>
              )}

              {serverAgents.some((a) => a.connected && !localAgentIds.includes(a.agent_id)) && (
                <Alert tone="info">
                  Online printers are already running on another computer. Connecting one here moves its printing to this computer.
                </Alert>
              )}
            </div>
          )}

          {/* ── Step 3 ── */}
          {step === "printer" && (
            <div className="grid grid-cols-1 xl:grid-cols-[minmax(0,1fr),minmax(0,1.1fr)] gap-8">
              <section className="space-y-5">
                {mode === "new" ? (
                  <>
                    <Field label="Send jobs to this printer by" required>
                      <div className="inline-flex rounded-lg border border-gray-800 p-0.5 bg-gray-900">
                        {(["category", "location"] as const).map((m) => (
                          <button
                            key={m}
                            type="button"
                            onClick={() => setRoutingMode(m)}
                            className={`px-4 py-1.5 rounded-md text-sm font-medium transition-colors ${
                              routingMode === m ? "bg-gray-700 text-white" : "text-gray-400 hover:text-gray-200"
                            }`}
                          >
                            {m === "category" ? "Main category" : "Table location"}
                          </button>
                        ))}
                      </div>
                    </Field>

                    <Field
                      label={routingMode === "location" ? "Locations" : "Categories"}
                      required
                      hint={
                        routingMode === "location"
                          ? "Every order from these locations prints here."
                          : "Items in these categories print here, e.g. Kitchen or Bar."
                      }
                    >
                      {routingLoading ? (
                        <div className="flex items-center gap-2 py-6 text-sm text-gray-500">
                          <Spinner /> Loading…
                        </div>
                      ) : routingOptions.length === 0 ? (
                        <input
                          className="input-field"
                          value={manualZones}
                          onChange={(e) => {
                            setManualZones(e.target.value);
                            if (!deviceNameTouched) {
                              setDeviceName(suggestDeviceName(e.target.value.split(",").map((s) => s.trim()).filter(Boolean)));
                            }
                          }}
                          placeholder={routingMode === "location" ? "e.g. Ground Floor, Patio" : "e.g. Kitchen, Bar"}
                        />
                      ) : (
                        <div className="rounded-xl border border-gray-800 overflow-hidden">
                          <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-800 bg-gray-900">
                            <Icon name="search" className="w-3.5 h-3.5 text-gray-500" />
                            <input
                              className="flex-1 bg-transparent text-sm outline-none placeholder-gray-600"
                              placeholder="Search…"
                              value={routingQuery}
                              onChange={(e) => setRoutingQuery(e.target.value)}
                            />
                            <span className="text-xs text-gray-500">{routingIds.length} selected</span>
                            {freeOptions.length > 0 && (
                              <button
                                type="button"
                                className="text-xs text-green-400 hover:text-green-300"
                                onClick={() => {
                                  const all = freeOptions.map((o) => o._id);
                                  setRoutingIds(all);
                                  if (!deviceNameTouched) setDeviceName(suggestDeviceName(freeOptions.map((o) => o.name)));
                                }}
                              >
                                All free
                              </button>
                            )}
                          </div>
                          <div className="max-h-64 overflow-y-auto p-2 grid grid-cols-1 sm:grid-cols-2 gap-1.5">
                            {filteredOptions.map((o) => {
                              const owner = optionOwner(o);
                              const checked = routingIds.includes(o._id);
                              return (
                                <label
                                  key={o._id}
                                  title={owner ? `Already printed by ${owner}` : undefined}
                                  className={`flex items-center gap-2.5 rounded-lg px-3 py-2 text-sm border transition-colors ${
                                    owner
                                      ? "border-transparent text-gray-600 cursor-not-allowed"
                                      : checked
                                        ? "border-green-500/40 bg-green-500/10 text-white cursor-pointer"
                                        : "border-transparent text-gray-300 hover:bg-white/[0.04] cursor-pointer"
                                  }`}
                                >
                                  <input
                                    type="checkbox"
                                    className="accent-green-500"
                                    disabled={!!owner}
                                    checked={checked}
                                    onChange={() => toggleRouting(o._id)}
                                  />
                                  <span className="flex-1 truncate">{o.name}</span>
                                  {owner && <span className="text-[10px] text-gray-500 truncate max-w-[45%]">{owner}</span>}
                                </label>
                              );
                            })}
                            {filteredOptions.length === 0 && (
                              <div className="col-span-full py-4 text-center text-xs text-gray-500">No matches</div>
                            )}
                          </div>
                        </div>
                      )}
                    </Field>

                    <Field label="Printer name" required hint="Shown in the POS, e.g. “Kitchen Printer”.">
                      <input
                        className="input-field"
                        value={deviceName}
                        onChange={(e) => {
                          setDeviceNameTouched(true);
                          setDeviceName(e.target.value);
                        }}
                        placeholder="e.g. Kitchen Printer"
                      />
                    </Field>
                  </>
                ) : (
                  selectedAgent && (
                    <div className="rounded-xl border border-gray-800 bg-gray-900 p-5 space-y-3">
                      <div className="flex items-center gap-3">
                        <span className="w-10 h-10 rounded-xl bg-gray-800 text-gray-300 flex items-center justify-center">
                          <Icon name="printer" className="w-5 h-5" />
                        </span>
                        <div>
                          <div className="text-sm font-medium text-white">{selectedAgent.device_name}</div>
                          <div className="text-xs text-gray-500">{branch?.name}</div>
                        </div>
                      </div>
                      <div className="flex flex-wrap gap-1.5">
                        {agentZones(selectedAgent).map((z) => (
                          <Badge key={z}>{z}</Badge>
                        ))}
                      </div>
                      <p className="text-xs text-gray-400">
                        Routing stays the same. Choose the printer on this computer that should print these jobs.
                      </p>
                    </div>
                  )
                )}
              </section>

              <section>
                <h3 className="text-sm font-medium text-gray-300 mb-3">Printer on this computer</h3>
                <PrinterPicker value={printer} onChange={setPrinter} />
              </section>
            </div>
          )}
        </div>

        {/* ── Footer ── */}
        <footer className="border-t border-gray-800 px-8 py-4 space-y-3">
          {formError && step === "printer" && (
            <Alert tone="error" onClose={() => setFormError(null)}>
              {formError}
            </Alert>
          )}
          <div className="flex items-center justify-between gap-3">
            {step === "connect" ? (
              <span />
            ) : (
              <Button
                variant="ghost"
                icon="arrowLeft"
                onClick={() => setStep(step === "printer" ? "choose" : "connect")}
                disabled={saving}
              >
                Back
              </Button>
            )}
            {step === "connect" && (
              <Button variant="primary" size="lg" disabled={!branchId} onClick={goChoose}>
                Continue
                <Icon name="arrowRight" />
              </Button>
            )}
            {step === "printer" && (
              <Button
                variant="primary"
                size="lg"
                loading={saving}
                disabled={!printerIsReady(printer)}
                onClick={submit}
              >
                {mode === "existing" ? "Connect printer" : "Pair printer"}
              </Button>
            )}
          </div>
        </footer>
      </main>
    </div>
  );
}
