import { useEffect, useState } from "react";
import { invoke } from "./lib/invoke";
import SetupScreen from "./screens/SetupScreen";
import StatusScreen from "./screens/StatusScreen";

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
}

interface Config {
  agents: AgentConfig[];
}

export default function App() {
  const [shopId, setShopId] = useState<string>("");
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<Config>("get_config")
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

  if (!config || config.agents.length === 0) {
    return <SetupScreen onPaired={(branchId) => {
    setShopId(branchId);
    invoke<Config>("get_config").then(setConfig);
  }} />;
  }

  return <StatusScreen config={config} shopId={shopId} onShowSetup={() => setConfig(null)} />;
}
