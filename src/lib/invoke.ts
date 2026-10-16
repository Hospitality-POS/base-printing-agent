import { invoke as tauriInvoke } from "@tauri-apps/api/core";

const isTauri = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (isTauri()) {
    return tauriInvoke<T>(cmd, args as never);
  }
  return webInvoke<T>(cmd, args ?? {});
}

// ---------- Web (browser) fallback ----------

const LIVE_API_URL = "https://api.hospitality.reliatech.co.ke";

const API_BASE: string =
  (import.meta.env.VITE_PRINT_AGENT_API_URL as string | undefined) ??
  "/agent-api";

// Local native helper (print-agent-helper.exe) that performs printing and
// hardware access for the browser UI — needed on Windows 7/8/8.1 where the
// Tauri desktop app cannot run.
const HELPER_BASE: string =
  (import.meta.env.VITE_PRINT_AGENT_HELPER_URL as string | undefined) ??
  "http://127.0.0.1:14200";

async function helperInvoke<T>(
  cmd: string,
  args: Record<string, unknown>
): Promise<T> {
  const resp = await fetch(`${HELPER_BASE}/cmd`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ cmd, args }),
  });
  const data = await resp.json();
  if (!data.ok) throw new Error(data.error ?? `Helper command "${cmd}" failed`);
  return data.result as T;
}

async function helperList<T>(cmd: string): Promise<T> {
  try {
    return await helperInvoke<T>(cmd, {});
  } catch {
    // Helper not running — behave like no hardware found
    return [] as T;
  }
}

const CONFIG_KEY = "print-agent-web-config";

interface AgentConfig {
  agentId: string;
  companyId: string;
  companyCode: string;
  branchId: string;
  zone: string;
  deviceName: string;
  deviceToken: string;
  printerPort: string;
  paperWidth: number;
  wsUrl: string;
  printerType: string;
  printerHost?: string;
  printerNetworkPort?: number;
  routingMode?: string;
  mainCategoryId?: string;
  location?: string;
  zones?: string[];
  mainCategoryIds?: string[];
  locations?: string[];
}

interface Config {
  agents: AgentConfig[];
  apiUrl?: string;
}

function loadConfig(): Config {
  try {
    const raw = localStorage.getItem(CONFIG_KEY);
    if (raw) {
      const cfg = JSON.parse(raw) as Config;
      // Keep the last entry per agentId so the same agent never shows twice.
      const byId = new Map<string, AgentConfig>();
      for (const a of cfg.agents ?? []) byId.set(a.agentId, a);
      cfg.agents = [...byId.values()];
      return cfg;
    }
  } catch {
    // fall through
  }
  return { agents: [] };
}

/** Insert or replace by agentId — never duplicates. */
function upsertAgent(cfg: Config, agent: AgentConfig) {
  const i = cfg.agents.findIndex((a) => a.agentId === agent.agentId);
  if (i >= 0) cfg.agents[i] = agent;
  else cfg.agents.push(agent);
}

function saveConfig(cfg: Config) {
  localStorage.setItem(CONFIG_KEY, JSON.stringify(cfg));
}

function normalizeApiUrl(url?: string | null) {
  const cleaned = url?.trim().replace(/\/+$/, "");
  return cleaned || undefined;
}

function configuredApiUrl() {
  return normalizeApiUrl(loadConfig().apiUrl);
}

function envApiUrl() {
  return normalizeApiUrl(
    import.meta.env.VITE_PRINT_AGENT_API_URL as string | undefined
  );
}

function isHelperOrigin() {
  try {
    const helper = new URL(HELPER_BASE);
    if (window.location.origin === helper.origin) return true;
    const localHosts = new Set(["localhost", "127.0.0.1"]);
    return (
      window.location.port === helper.port &&
      localHosts.has(window.location.hostname) &&
      localHosts.has(helper.hostname)
    );
  } catch {
    return false;
  }
}

function apiUrl(path: string) {
  const configured = configuredApiUrl();
  // When the page is served by the local helper, keep requests same-origin so
  // the helper can proxy to whichever API URL was selected during setup.
  if (configured && isHelperOrigin()) return `/agent-api${path}`;
  return `${configured ?? API_BASE}${path}`;
}

async function apiFetch(
  path: string,
  init: RequestInit = {},
  companyCode?: string
): Promise<Response> {
  const headers = new Headers(init.headers);
  if (companyCode) headers.set("companycode", companyCode);
  const resp = await fetch(apiUrl(path), { ...init, headers });
  return resp;
}

async function ensureOk(resp: Response, label: string) {
  if (!resp.ok) {
    const text = await resp.text().catch(() => "");
    throw new Error(`${label} failed (${resp.status}): ${text}`);
  }
}

/** Deactivate an agent on the API. Returns a warning string on failure, else null. */
async function unpairOnApi(
  agentId: string,
  companyCode: string
): Promise<string | null> {
  try {
    const resp = await apiFetch(
      `/api/agents/${encodeURIComponent(agentId)}/unpair`,
      { method: "DELETE" },
      companyCode
    );
    if (resp.ok || resp.status === 404) return null;
    const text = await resp.text().catch(() => "");
    return `Removed locally, but the server could not be updated (${resp.status}: ${text})`;
  } catch (e) {
    return `Removed locally, but the server could not be updated (${
      e instanceof Error ? e.message : String(e)
    })`;
  }
}

async function apiGetJson(
  path: string,
  companyCode: string | undefined,
  label: string
) {
  const resp = await apiFetch(path, {}, companyCode);
  await ensureOk(resp, label);
  return resp.json();
}

async function webInvoke<T>(
  cmd: string,
  args: Record<string, unknown>
): Promise<T> {
  switch (cmd) {
    case "get_config":
      return loadConfig() as T;

    case "get_api_url": {
      const configured = configuredApiUrl();
      try {
        const helperUrl = await helperInvoke<string>("get_api_url", {});
        const normalized = normalizeApiUrl(helperUrl);
        const cfg = loadConfig();
        if (normalized) cfg.apiUrl = normalized;
        else delete cfg.apiUrl;
        saveConfig(cfg);
        return (normalized ?? LIVE_API_URL) as T;
      } catch {
        return (configured ?? envApiUrl() ?? LIVE_API_URL) as T;
      }
    }

    case "set_api_url": {
      const normalized = normalizeApiUrl(args.url as string | undefined);
      const cfg = loadConfig();
      if (normalized) cfg.apiUrl = normalized;
      else delete cfg.apiUrl;
      saveConfig(cfg);
      try {
        await helperInvoke("set_api_url", { url: normalized ?? null });
      } catch {
        // The helper only exists in the installed legacy flow; browser/dev
        // mode can still use the value stored in localStorage.
      }
      return undefined as T;
    }

    case "get_agent_status": {
      const cfg = loadConfig();
      return cfg.agents.map((a) => ({
        agentId: a.agentId,
        wsConnected: false,
        zone: a.zone,
        branchId: a.branchId,
        deviceName: a.deviceName,
        queueCount: 0,
      })) as T;
    }

    case "fetch_shops": {
      const data = await apiGetJson(
        `/api/shops`,
        args.companyCode as string,
        "Fetch shops"
      );
      return JSON.stringify(data) as T;
    }

    case "list_agents": {
      let path = `/api/agents/list?shop_id=${encodeURIComponent(
        args.branchId as string
      )}`;
      if (args.zone) path += `&zone=${encodeURIComponent(args.zone as string)}`;
      const data = await apiGetJson(
        path,
        args.companyCode as string,
        "List agents"
      );
      return JSON.stringify(data.agents ?? data) as T;
    }

    case "fetch_connected_agents": {
      const data = await apiGetJson(
        `/api/agents/connected?shop_id=${encodeURIComponent(
          args.shopId as string
        )}`,
        args.companyCode as string,
        "Fetch connected agents"
      );
      return JSON.stringify(data) as T;
    }

    case "fetch_agents_from_api": {
      const data = await apiGetJson(
        `/api/agents/list?shop_id=${encodeURIComponent(args.shopId as string)}`,
        args.companyCode as string,
        "Fetch agents"
      );
      return (data.agents ?? data) as T;
    }

    case "fetch_categories":
    case "fetch_locations": {
      const envVar =
        cmd === "fetch_categories"
          ? (import.meta.env.VITE_PRINT_AGENT_CATEGORIES_PATH as string | undefined)
          : (import.meta.env.VITE_PRINT_AGENT_LOCATIONS_PATH as string | undefined);
      const path =
        envVar ??
        (cmd === "fetch_categories"
          ? "/categories/main-categories"
          : "/tables/location/locations");
      const sep = path.includes("?") ? "&" : "?";
      try {
        const resp = await apiFetch(
          `${path}${sep}shop_id=${encodeURIComponent(args.shopId as string)}`,
          {},
          args.companyCode as string
        );
        await ensureOk(resp, cmd);
        return (await resp.text()) as T;
      } catch {
        // Fallback: derive distinct values from pending jobs
        const field = cmd === "fetch_categories" ? "zone" : "location";
        const resp = await apiFetch(
          `/api/agents/pending-jobs?shop_id=${encodeURIComponent(args.shopId as string)}`,
          {},
          args.companyCode as string
        );
        await ensureOk(resp, "Fetch pending jobs");
        const body = await resp.json();
        const jobs: any[] = body?.jobs ?? [];
        const seen = new Set<string>();
        for (const j of jobs) {
          const v = j?.[field];
          if (typeof v === "string" && v) seen.add(v);
        }
        const options = [...seen].sort().map((v) => ({ _id: v, name: v }));
        return JSON.stringify(options) as T;
      }
    }

    case "fetch_pending_jobs": {
      let path = `/api/agents/pending-jobs?shop_id=${encodeURIComponent(
        args.shopId as string
      )}`;
      if (args.zone) path += `&zone=${encodeURIComponent(args.zone as string)}`;
      if (args.status)
        path += `&status=${encodeURIComponent(args.status as string)}`;
      const resp = await apiFetch(path);
      await ensureOk(resp, "Fetch pending jobs");
      return (await resp.text()) as T;
    }

    case "pair_device": {
      const cfg = loadConfig();
      const existing = cfg.agents.find(
        (a) => a.zone === args.zone && a.branchId === args.branchId
      );
      if (existing) {
        const selectedApi = configuredApiUrl();
        if (selectedApi && selectedApi !== LIVE_API_URL) {
          existing.wsUrl = `${selectedApi.replace(/^http/, "ws")}/ws/agent`;
        }
        existing.deviceName = (args.deviceName as string) ?? existing.deviceName;
        existing.printerPort = (args.printerPort as string) ?? "";
        existing.printerType = (args.printerType as string) ?? "serial";
        existing.printerHost = args.printerHost as string | undefined;
        existing.printerNetworkPort = args.printerNetworkPort as
          | number
          | undefined;
        existing.zones = (args.zones as string[]) ?? existing.zones;
        existing.mainCategoryIds =
          (args.mainCategoryIds as string[]) ?? existing.mainCategoryIds;
        existing.locations = (args.locations as string[]) ?? existing.locations;
        saveConfig(cfg);
        return undefined as T;
      }

      const body = {
        companyCode: args.companyCode,
        branchId: args.branchId,
        zone: args.zone,
        deviceName: args.deviceName,
        printerType: args.printerType,
        routingMode: args.routingMode,
        mainCategoryId: args.mainCategoryId,
        location: args.location,
        zones: args.zones,
        mainCategoryIds: args.mainCategoryIds,
        locations: args.locations,
      };
      const resp = await apiFetch(`/api/agents/pair`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (resp.status === 409) {
        const data = await resp.json().catch(() => ({}));
        throw new Error(
          `${data?.error ?? `An agent is already paired for "${args.zone}"`}. ` +
            `To print from this computer, choose it under "Existing printers" instead of adding a new one.`
        );
      }
      await ensureOk(resp, "Pairing");
      const pair = await resp.json();
      const selectedApi = configuredApiUrl() ?? LIVE_API_URL;
      const wsUrl =
        selectedApi === LIVE_API_URL
          ? pair.wsUrl ?? ""
          : `${selectedApi.replace(/^http/, "ws")}/ws/agent`;

      upsertAgent(cfg, {
        agentId: pair.agentId,
        companyId: pair.companyId,
        companyCode: args.companyCode as string,
        branchId: args.branchId as string,
        zone: args.zone as string,
        deviceName: args.deviceName as string,
        deviceToken: pair.deviceToken ?? "",
        printerPort: (args.printerPort as string) ?? "",
        paperWidth: 80,
        wsUrl,
        printerType: (args.printerType as string) ?? "serial",
        printerHost: args.printerHost as string | undefined,
        printerNetworkPort: args.printerNetworkPort as number | undefined,
        routingMode: args.routingMode as string | undefined,
        mainCategoryId: args.mainCategoryId as string | undefined,
        location: args.location as string | undefined,
        zones: (args.zones as string[]) ?? [args.zone as string],
        mainCategoryIds: (args.mainCategoryIds as string[]) ?? [],
        locations: (args.locations as string[]) ?? [],
      });
      saveConfig(cfg);
      return undefined as T;
    }

    case "auto_add_connected_agent": {
      const cfg = loadConfig();
      const agentId = args.agentId as string;
      const existing = cfg.agents.find((a) => a.agentId === agentId);
      if (existing) {
        if (!existing.branchId) existing.branchId = args.shopId as string;
        if (!existing.zone) existing.zone = args.zone as string;
        saveConfig(cfg);
        return undefined as T;
      }

      let printerPort = "";
      let printerType = "serial";
      let printerHost: string | undefined;
      let printerNetworkPort: number | undefined;
      try {
        const details = await apiGetJson(
          `/api/agents/${encodeURIComponent(agentId)}`,
          args.companyCode as string,
          "Fetch agent"
        );
        printerPort = details.printer_port ?? "";
        printerType = details.printer_type ?? "serial";
        printerHost = details.printer_host ?? undefined;
        printerNetworkPort = details.printer_network_port ?? undefined;
      } catch {
        // ignore — defaults are fine
      }

      upsertAgent(cfg, {
        agentId,
        companyId: (args.companyId as string) ?? "",
        companyCode: (args.companyCode as string) ?? "",
        branchId: (args.shopId as string) ?? "",
        zone: (args.zone as string) ?? "",
        deviceName: (args.deviceName as string) ?? "",
        deviceToken: "",
        printerPort,
        paperWidth: 80,
        wsUrl: "",
        printerType,
        printerHost,
        printerNetworkPort,
      });
      saveConfig(cfg);
      return undefined as T;
    }

    case "connect_to_existing_agent": {
      const cfg = loadConfig();
      const prev = cfg.agents.find((a) => a.agentId === args.agentId);
      const pick = (v: unknown, old?: string) =>
        (typeof v === "string" && v) || old || "";
      const zone = pick(args.zone, prev?.zone);
      const passedZones = (args.zones as string[] | undefined) ?? [];
      const agent: AgentConfig = {
        ...prev,
        agentId: args.agentId as string,
        companyId: pick(args.companyId, prev?.companyId),
        companyCode: (args.companyCode as string) ?? prev?.companyCode ?? "",
        branchId: pick(args.shopId, prev?.branchId),
        zone,
        zones: passedZones.length ? passedZones : prev?.zones?.length ? prev.zones : zone ? [zone] : [],
        routingMode: pick(args.routingMode, prev?.routingMode) || undefined,
        deviceName: pick(args.deviceName, prev?.deviceName),
        deviceToken: prev?.deviceToken ?? "",
        printerPort: (args.printerPort as string) ?? "",
        paperWidth: 80,
        wsUrl: prev?.wsUrl ?? "",
        printerType: (args.printerType as string) ?? "serial",
        printerHost: args.printerHost as string | undefined,
        printerNetworkPort: args.printerNetworkPort as number | undefined,
      };

      // Sync printer settings first; never send blank routing to the server.
      const resp = await apiFetch(
        `/api/agents/${encodeURIComponent(agent.agentId)}/update`,
        {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            ...(agent.branchId && { branch_id: agent.branchId }),
            ...(agent.zone && { zone: agent.zone }),
            printer_type: agent.printerType,
            printer_port: agent.printerPort,
            printer_host: agent.printerHost,
            printer_network_port: agent.printerNetworkPort,
          }),
        },
        agent.companyCode
      );
      await ensureOk(resp, "Sync agent to API");

      upsertAgent(cfg, agent);
      saveConfig(cfg);
      return undefined as T;
    }

    case "update_agent_printer_port": {
      const deviceName = ((args.deviceName as string | undefined) ?? "").trim();
      const zone = ((args.zone as string | undefined) ?? "").trim();
      const resp = await apiFetch(
        `/api/agents/${encodeURIComponent(args.agentId as string)}/update`,
        {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            printer_port: args.printerPort,
            printer_type: args.printerType,
            printer_host: args.printerHost,
            printer_network_port: args.printerNetworkPort,
            ...(deviceName && { device_name: deviceName }),
            ...(zone && { zone, zones: [zone] }),
          }),
        },
        args.companyCode as string
      );
      await ensureOk(resp, "Update agent");

      const cfg = loadConfig();
      const agent = cfg.agents.find((a) => a.agentId === args.agentId);
      if (agent) {
        if (deviceName) agent.deviceName = deviceName;
        if (zone) {
          agent.zone = zone;
          agent.zones = [zone];
        }
        agent.printerPort = (args.printerPort as string) ?? "";
        agent.printerType = (args.printerType as string) ?? agent.printerType;
        agent.printerHost = args.printerHost as string | undefined;
        agent.printerNetworkPort = args.printerNetworkPort as
          | number
          | undefined;
        saveConfig(cfg);
      }
      return undefined as T;
    }

    case "unpair_device": {
      const cfg = loadConfig();
      const target = cfg.agents.find((a) => a.agentId === args.agentId);
      const warning = await unpairOnApi(
        args.agentId as string,
        target?.companyCode ?? ""
      );
      cfg.agents = cfg.agents.filter((a) => a.agentId !== args.agentId);
      saveConfig(cfg);
      return warning as T;
    }

    case "unpair_all_agents": {
      const cfg = loadConfig();
      const count = cfg.agents.length;
      let failures = 0;
      for (const a of cfg.agents) {
        if (await unpairOnApi(a.agentId, a.companyCode)) failures++;
      }
      cfg.agents = [];
      saveConfig(cfg);
      return (failures > 0
        ? `Unpaired ${count} agent(s) locally; ${failures} could not be removed on the server`
        : `Unpaired ${count} agent(s)`) as T;
    }

    case "remove_duplicate_agents": {
      const cfg = loadConfig();
      const seen = new Set<string>();
      const before = cfg.agents.length;
      cfg.agents = cfg.agents.filter((a) =>
        seen.has(a.agentId) ? false : (seen.add(a.agentId), true)
      );
      saveConfig(cfg);
      return (before - cfg.agents.length) as T;
    }

    case "get_os_version": {
      try {
        return await helperInvoke<T>(cmd, args);
      } catch {
        // Fall back to what the browser can tell us
        const m = navigator.userAgent.match(/Windows NT ([\d.]+)/);
        const [major = 10, minor = 0, build = 0] = m
          ? m[1].split(".").map(Number)
          : [];
        const name =
          major === 10 && build >= 22000
            ? "Windows 11"
            : major === 10
              ? "Windows 10"
              : major === 6 && minor === 3
                ? "Windows 8.1"
                : major === 6 && minor === 2
                  ? "Windows 8"
                  : major === 6 && minor === 1
                    ? "Windows 7"
                    : "Windows";
        return {
          major,
          minor,
          build,
          name,
          supportsDesktopApp: major >= 10,
          supportsHelper: major > 6 || (major === 6 && minor >= 1),
        } as T;
      }
    }

    case "get_available_ports":
    case "detect_thermal_printers":
    case "list_windows_printers":
    case "discover_network_printers":
      return helperList<T>(cmd);

    case "test_print":
    case "test_printer_connection":
    case "set_autostart":
      return helperInvoke<T>(cmd, args);

    case "get_autostart": {
      try {
        return await helperInvoke<T>(cmd, args);
      } catch {
        return false as T;
      }
    }

    default:
      throw new Error(`Command "${cmd}" is not supported in the browser version`);
  }
}
