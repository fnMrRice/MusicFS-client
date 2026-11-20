import { useCallback, useEffect, useRef, useState } from "react";
import { desktop, type PlayerSnapshot, type Track } from "../bridge";
import { message } from "./Connection";

export function usePlayer() {
  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);
  const [error, setError] = useState("");
  const alive = useRef(false);
  const revision = useRef(0);
  const refresh = useCallback(async () => {
    const request = ++revision.current;
    try {
      const value = await desktop.snapshot();
      if (alive.current && request === revision.current) setSnapshot(value);
    } catch (reason) {
      if (alive.current && request === revision.current) setError(message(reason));
    }
  }, []);
  useEffect(() => {
    alive.current = true;
    let timer: ReturnType<typeof setTimeout>;
    let stopped = false;
    async function update() {
      if (!document.hidden) await refresh();
      if (!stopped) timer = setTimeout(update, 600);
    }
    void update();
    const onVisible = () => { if (!document.hidden) void refresh(); };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      stopped = true;
      alive.current = false;
      revision.current++;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [refresh]);
  const run = async (operation: () => Promise<unknown>) => {
    setError("");
    try { await operation(); }
    catch (reason) { if (alive.current) setError(message(reason)); }
    finally { if (alive.current) await refresh(); }
  };
  const play = (items: Track[], index: number) => {
    if (!items[index]?.sourceTrackId) return;
    const tracks = items.filter(item => item.sourceTrackId);
    const start = items.slice(0, index).filter(item => item.sourceTrackId).length;
    void run(() => desktop.play(tracks, start));
  };
  return { snapshot, error, clearError: () => setError(""), run, play };
}
export type Player = ReturnType<typeof usePlayer>;
