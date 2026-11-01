import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import type { RunningGame } from "../lib/types";

export type RunningMap = Record<string, RunningGame>;

/**
 * Live game processes, keyed by instance id.
 *
 * `game://changed` makes start/stop feel instant; the interval is the safety
 * net for a game that exits on its own or crashes, which no event announces.
 */
export function useRunning() {
  const [running, setRunning] = useState<RunningMap>({});

  const refresh = useCallback(async () => {
    try {
      const list = await api.instances.running();
      setRunning(Object.fromEntries(list.map((game) => [game.instance_id, game])));
    } catch {
      // During shutdown the backend can disappear before the webview does.
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    void refresh();

    const timer = window.setInterval(() => {
      if (!disposed) void refresh();
    }, 2500);

    const unlisten = listen("game://changed", () => {
      if (!disposed) void refresh();
    });

    return () => {
      disposed = true;
      window.clearInterval(timer);
      void unlisten.then((off) => off());
    };
  }, [refresh]);

  return { running, refresh };
}
