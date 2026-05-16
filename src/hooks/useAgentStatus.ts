import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AgentStatus {
  agentId: string;
  wsConnected: boolean;
  zone: string;
  branchId: string;
  deviceName: string;
  queueCount: number;
}

export function useAgentStatus() {
  const [statuses, setStatuses] = useState<AgentStatus[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const result = await invoke<AgentStatus[]>("get_agent_status");
        setStatuses(result);
      } catch (e) {
        console.error("Failed to fetch agent status:", e);
      } finally {
        setLoading(false);
      }
    };

    fetchStatus();
    const interval = setInterval(fetchStatus, 2000);
    return () => clearInterval(interval);
  }, []);

  return { statuses, loading };
}
