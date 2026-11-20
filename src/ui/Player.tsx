import { useEffect, useRef, useState, type CSSProperties } from "react";
import { desktop } from "../bridge";
import { Icon, IconButton } from "./Icons";
import type { Player } from "./usePlayer";

export function time(ms: number | null | undefined) {
  if (ms == null || !Number.isFinite(ms)) return "–:––";
  const seconds = Math.floor(Math.max(0, ms) / 1000);
  return Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0");
}

function Cover({ id }: { id?: string | null }) {
  const [image, setImage] = useState<{ id: string; url: string } | null>(null);
  useEffect(() => {
    let current = true;
    if (id) void desktop.cover(id).then(url => { if (current) setImage({ id, url }); }, () => {});
    return () => { current = false; };
  }, [id]);
  return <div className="cover">{id && image?.id === id ? <img alt="" src={image.url} /> : <Icon name="disc" size={32} />}</div>;
}

export function PlayerBar({ player }: { player: Player }) {
  const { snapshot, run } = player;
  const [queueOpen, setQueueOpen] = useState(false);
  const [seek, setSeek] = useState<number | null>(null);
  const [volumeDraft, setVolumeDraft] = useState<number | null>(null);
  const lastLoud = useRef(0.7);
  const panel = useRef<HTMLDialogElement>(null);
  const index = snapshot?.index ?? null;
  const tracks = snapshot?.queue ?? [];
  const current = index == null ? null : tracks[index];
  const duration = snapshot?.durationMs ?? current?.durationMs ?? null;
  const position = seek ?? snapshot?.positionMs ?? 0;
  const volume = volumeDraft ?? snapshot?.volume ?? 1;
  const playing = snapshot?.status === "playing";
  const loading = snapshot?.status === "loading";
  const playOrPause = () => void run(() => desktop.control(playing ? "pause" : "resume"));
  useEffect(() => {
    setSeek(null);
  }, [index, current?.id, current?.sourceTrackId]);
  useEffect(() => {
    const dialog = panel.current;
    if (queueOpen) dialog?.showModal();
    else if (dialog?.open) dialog.close();
  }, [queueOpen]);
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      const target = event.target;
      if (event.code !== "Space" || event.repeat || event.altKey || event.ctrlKey || event.metaKey || !current || loading || queueOpen) return;
      if (target instanceof HTMLElement && (target.isContentEditable || target.closest("input, button, select, textarea, a, dialog"))) return;
      event.preventDefault();
      playOrPause();
    };
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  });
  const setVolume = (value: number) => {
    if (value > 0) lastLoud.current = value;
    void run(() => desktop.volume(value)).finally(() => setVolumeDraft(null));
  };
  const commitSeek = (value: number) => {
    if (!snapshot?.seekable) return;
    const range = snapshot.seekRangeMs ?? [0, duration || value];
    void run(() => desktop.seek(Math.max(range[0], Math.min(range[1], value)))).finally(() => setSeek(null));
  };
  const rangeKeys = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End"];
  const error = player.error || snapshot?.error;
  return <footer className="player" aria-label="播放器">
    <input className="timeline" aria-label="曲目进度" aria-valuetext={time(position)}
      style={{ "--progress": `${duration ? Math.min(position / duration * 100, 100) : 0}%` } as CSSProperties}
      type="range" min={0} max={duration || 1} step={100} value={Math.min(position, duration || 1)}
      disabled={!snapshot?.seekable || !duration} onPointerCancel={() => setSeek(null)}
      onChange={e => setSeek(Number(e.target.value))} onPointerUp={e => commitSeek(Number(e.currentTarget.value))}
      onKeyUp={e => { if (rangeKeys.includes(e.key)) commitSeek(Number(e.currentTarget.value)); }} />
    <div className="now-playing">
      <Cover id={current?.coverObjectId} />
      <div className="track-info"><strong>{current?.title || "暂无音频"}</strong>
        <small>{loading ? "正在加载…" : current ? current.artists.join(" / ") || "未知歌手" : "尚未选择曲目"}</small>
      </div>
    </div>
    <div className="transport">
      <IconButton icon="previous" label="上一首" disabled={index == null || index <= 0} onClick={() => void run(() => desktop.control("previous"))} />
      <IconButton icon={playing ? "pause" : "play"} label={playing ? "暂停" : "播放"} className="primary main-play"
        disabled={!current || loading} onClick={playOrPause} />
      <IconButton icon="next" label="下一首" disabled={index == null || index >= tracks.length - 1} onClick={() => void run(() => desktop.control("next"))} />
      <span className="time">{time(position)} <span>/ {time(duration)}</span></span>
    </div>
    <div className="player-tools">
      <IconButton icon="lyrics" label="桌面歌词" onClick={() => void run(() => desktop.openWindow("lyrics"))} />
      <IconButton icon="mini" label="迷你播放器" onClick={() => void run(() => desktop.openWindow("mini"))} />
      <IconButton icon={volume === 0 ? "mute" : "volume"} label={volume === 0 ? "取消静音" : "静音"} disabled={!snapshot}
        onClick={() => setVolume(volume > 0 ? 0 : lastLoud.current)} />
      <input className="volume" type="range" aria-label="音量" aria-valuetext={Math.round(volume * 100) + "%"} disabled={!snapshot}
        min={0} max={100} value={Math.round(volume * 100)} onChange={e => setVolumeDraft(Number(e.target.value) / 100)}
        onPointerCancel={() => setVolumeDraft(null)} onPointerUp={e => setVolume(Number(e.currentTarget.value) / 100)}
        onKeyUp={e => { if (rangeKeys.includes(e.key)) setVolume(Number(e.currentTarget.value) / 100); }} />
      <IconButton icon="queue" label="打开播放队列" aria-haspopup="dialog" aria-expanded={queueOpen} onClick={() => setQueueOpen(true)} />
    </div>
    {error && <div role="alert" className="player-error"><span>{error}</span>
      {snapshot?.error && index !== null && <button onClick={() => void run(() => desktop.select(index))}>重新播放</button>}
      {player.error && <IconButton icon="close" label="关闭提示" onClick={player.clearError} />}
    </div>}
    <dialog className="queue-panel" ref={panel} aria-labelledby="queue-title" onClose={() => setQueueOpen(false)}
      onClick={e => { if (e.target === e.currentTarget) { const r = e.currentTarget.getBoundingClientRect(); if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) setQueueOpen(false); } }}>
      <div className="panel-head"><h2 id="queue-title">播放队列 <small>{tracks.length}</small></h2>
        <IconButton icon="clear" label="清空队列" disabled={!tracks.length} onClick={() => void run(() => desktop.control("clear"))} />
        <IconButton icon="close" label="关闭队列" onClick={() => setQueueOpen(false)} /></div>
      {tracks.length ? <ol className="queue-list">{tracks.map((track, i) => <li key={i}>
        <button className={i === index ? "selected" : ""} aria-current={i === index ? "true" : undefined} onClick={() => void run(() => desktop.select(i))}>
          <span className="row-number">{i === index ? <Icon name="music" size={17} /> : i + 1}</span>
          <span className="track-info"><strong>{track.title}</strong><small>{track.artists.join(" / ") || "未知歌手"}</small></span>
          <span className="time">{time(track.durationMs)}</span></button>
      </li>)}</ol> : <div className="empty small"><Icon name="queue" size={32} /><h3>队列为空</h3><p>从曲目列表选择一首音乐开始播放。</p></div>}
    </dialog>
  </footer>;
}
