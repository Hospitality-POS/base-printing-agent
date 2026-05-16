import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface AgentStatus {
  wsConnected: boolean;
  zone: string;
  branchId: string;
  lastJobId: string | null;
  lastJobTime: string | null;
  queueCount: number;
}

export function useAgentStatus() {
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;

    const poll = async () => {
      try {
        const s = await invoke<AgentStatus>("get_agent_status");
        if (active) {
          setStatus(s);
          setLoading(false);
        }
      } catch {
        if (active) setLoading(false);
      }
    };

    poll();
    const interval = setInterval(poll, 2000);

    return () => {
      active = false;
      clearInterval(interval);
    };
  }, []);

  return { status, loading };
}
