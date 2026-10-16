import { useCallback, useEffect, useState } from "react";
import { invoke } from "./lib/invoke";
import SetupScreen from "./screens/SetupScreen";
import StatusScreen from "./screens/StatusScreen";
import { Spinner, ToastProvider } from "./components/ui";

type View = "loading" | "setup" | "status";

export default function App() {
  const [view, setView] = useState<View>("loading");
  const [hasAgents, setHasAgents] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const cfg = await invoke<{ agents: unknown[] }>("get_config");
      const any = (cfg?.agents?.length ?? 0) > 0;
      setHasAgents(any);
      setView(any ? "status" : "setup");
    } catch {
      setHasAgents(false);
      setView("setup");
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const goSetup = useCallback(() => setView("setup"), []);
  const onEmpty = useCallback(() => {
    setHasAgents(false);
    setView("setup");
  }, []);

  return (
    <ToastProvider>
      {view === "loading" && (
        <div className="flex items-center justify-center h-screen bg-gray-950 gap-3 text-gray-400 text-sm">
          <Spinner className="w-5 h-5 text-green-500" />
          Starting PrintAgent…
        </div>
      )}
      {view === "setup" && (
        <SetupScreen onPaired={refresh} onCancel={hasAgents ? () => setView("status") : undefined} />
      )}
      {view === "status" && <StatusScreen onAddPrinter={goSetup} onEmpty={onEmpty} />}
    </ToastProvider>
  );
}
