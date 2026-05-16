import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import SetupScreen from "./screens/SetupScreen";
import StatusScreen from "./screens/StatusScreen";

interface Config {
  agentId: string;
  companyId: string;
  branchId: string;
  zone: string;
  deviceName: string;
  deviceToken: string;
  printerPort: string;
  paperWidth: number;
  wsUrl: string;
}

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<Config | null>("get_config")
      .then((cfg) => {
        setConfig(cfg);
      })
      .catch(() => {
        setConfig(null);
      })
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-screen bg-gray-950">
        <div className="flex flex-col items-center gap-3">
          <div className="w-8 h-8 border-2 border-green-500 border-t-transparent rounded-full animate-spin" />
          <span className="text-gray-400 text-sm">Starting PrintAgent...</span>
        </div>
      </div>
    );
  }

  if (!config) {
    return <SetupScreen onPaired={() => invoke<Config | null>("get_config").then(setConfig)} />;
  }

  return <StatusScreen onUnpair={() => setConfig(null)} />;
}
