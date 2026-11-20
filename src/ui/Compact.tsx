import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { desktop, type PlayerSnapshot } from "../bridge";
import { Icon, IconButton } from "./Icons";
import { time } from "./Player";
import { currentLine, parseLyrics } from "./lyrics";
import { useTheme } from "./preferences";
import "./compact.css";

interface Summary {
  instance: number;
  generation: number;
  title: string | null;
  artists: string[];
  sourceId: string | null;
  status: PlayerSnapshot["status"];
  positionMs: number;
  durationMs: number | null;
  hasNext: boolean;
  error: string | null;
}

function useSummary() {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let current = true;
    let timer = 0;
    const refresh = async () => {
      try {
        const value = await invoke<Summary | null>("compact_snapshot");
        if (current) { setSummary(value); setError(""); }
      } catch (reason) {
        if (current) { setSummary(null); setError(String(reason)); }
      } finally {
        if (current) timer = window.setTimeout(refresh, document.hidden ? 800 : 200);
      }
    };
    void refresh();
    return () => { current = false; window.clearTimeout(timer); };
  }, []);
  return { summary, error };
}

function Lyrics({ summary }: { summary: Summary | null }) {
  const source = summary?.sourceId;
  const instance = summary?.instance;
  const key = source ? instance + "/" + summary?.generation + "/" + source : "";
  const [retry, setRetry] = useState(0);
  const [result, setResult] = useState({ key: "", text: "", error: "" });
  useEffect(() => {
    if (!key) return;
    let current = true;
    void desktop.lyrics().then(text => {
      if (current) setResult({ key, text: text ?? "", error: "" });
    }, reason => {
      if (current) setResult({ key, text: "", error: String(reason) });
    });
    return () => { current = false; };
  }, [key, retry]);
  const text = key === result.key ? result.text.trim() : "";
  const lines = useMemo(() => parseLyrics(text), [text]);
  if (!source) return <p className="compact-hint">{summary?.title ? "等待播放…" : "尚未选择曲目"}</p>;
  if (result.key !== key) return <p className="compact-hint" role="status">正在读取歌词…</p>;
  if (result.error) return <div className="compact-hint" role="alert">{result.error}
    <IconButton icon="refresh" label="重试读取歌词" onClick={() => setRetry(v => v + 1)} /></div>;
  if (!text) return <p className="compact-hint">当前曲目暂无歌词</p>;
  if (!lines.length) return <div className="plain-lyrics" tabIndex={0}>{text}</div>;
  const index = currentLine(lines, summary?.positionMs ?? 0);
  return <div className="timed-lyrics">
    <p>{index < 0 ? "前奏" : lines[index].text || "间奏"}</p>
    {lines[index + 1]?.text && <span>{lines[index + 1].text}</span>}
  </div>;
}

export default function Compact({ kind }: { kind: "mini" | "lyrics" }) {
  const { summary, error } = useSummary();
  const [actionError, setActionError] = useState("");
  const [, setTheme] = useTheme();
  useEffect(() => {
    const changed = (event: StorageEvent) => {
      if (event.key === "musicfs.desktop.theme" && ["light", "dark", "system"].includes(event.newValue ?? "")) {
        setTheme(event.newValue as "light" | "dark" | "system");
      }
    };
    window.addEventListener("storage", changed);
    return () => window.removeEventListener("storage", changed);
  }, [setTheme]);
  const act = async (operation: () => Promise<unknown>) => {
    try { setActionError(""); await operation(); } catch (reason) { setActionError(String(reason)); }
  };
  const playing = summary?.status === "playing" || summary?.status === "loading";
  const issue = error || actionError || summary?.error;
  return <main className={"compact-window " + kind} aria-label={kind === "mini" ? "迷你播放器" : "桌面歌词"}>
    <header className="compact-head">
      <div className="compact-note"><Icon name={kind === "mini" ? "disc" : "music"} size={24} /></div>
      <div className="compact-track"><h1>{summary?.title || "MusicFS"}</h1>
        <p>{summary?.artists.join(" / ") || (summary?.title ? "未知歌手" : "尚未播放")}</p></div>
      <button type="button" className="compact-restore" onClick={() => void act(desktop.restoreMain)}>主窗口</button>
    </header>
    {kind === "lyrics" ? <Lyrics summary={summary} /> : <>
      <div className="compact-controls">
        <IconButton icon="previous" label="上一首" disabled={!summary?.title}
          onClick={() => void act(() => desktop.control("previous"))} />
        <IconButton icon={playing ? "pause" : "play"} label={playing ? "暂停" : "播放"} className="compact-play"
          disabled={!summary?.title} onClick={() => void act(() => desktop.control(playing ? "pause" : "play"))} />
        <IconButton icon="next" label="下一首" disabled={!summary?.hasNext}
          onClick={() => void act(() => desktop.control("next"))} />
        <span>{time(summary?.positionMs ?? 0)} / {time(summary?.durationMs)}</span>
      </div>
      <progress aria-label="播放进度" value={summary?.positionMs ?? 0} max={summary?.durationMs || 1} />
    </>}
    {issue && <p role="alert" className="compact-error">{issue}</p>}
  </main>;
}
