import { useEffect, useRef, useState, type FormEvent } from "react";
import { desktop, type Playlist, type Track } from "../bridge";
import { message, Notice } from "./Connection";
import { IconButton } from "./Icons";

export function PlaylistEditor({ track, playlists, close, saved }: {
  track: Track; playlists: Playlist[]; close: () => void; saved: () => void;
}) {
  const [id, setId] = useState(playlists[0]?.id ?? "");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const attempt = useRef<{ target: string; key: string } | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => { dialog.current?.showModal(); }, []);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    const existing = playlists.find(p => p.id === id);
    const target = existing ? { id: existing.id, ruleRevision: existing.ruleRevision } : { name: name.trim() };
    if (!existing && !name.trim()) return;
    const identity = JSON.stringify(target);
    if (attempt.current?.target !== identity) attempt.current = { target: identity, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try { await desktop.addToPlaylist(track, target, attempt.current.key); saved(); }
    catch (reason) { setError(message(reason)); setBusy(false); }
  }
  return <dialog className="add-dialog" ref={dialog} aria-labelledby="add-title" onCancel={e => { if (busy) e.preventDefault(); }} onClose={close}>
    <div className="panel-head"><h2 id="add-title">加入歌单</h2><IconButton icon="close" label="关闭" disabled={busy} onClick={close} /></div>
    <form onSubmit={submit}>
      <p className="adding-track"><strong>{track.title}</strong><span>{track.artists.join(" / ")}</span></p>
      <label htmlFor="target-list">选择歌单</label>
      <select id="target-list" disabled={busy} value={id} onChange={e => { setId(e.target.value); setError(""); }}>
        {playlists.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}<option value="">新建歌单…</option>
      </select>
      {!id && <><label htmlFor="new-playlist">歌单名称</label><input id="new-playlist" disabled={busy} autoComplete="off"
        required value={name} onChange={e => { setName(e.target.value); setError(""); }} /></>}
      {error && <Notice error={error} />}
      <button className="primary" disabled={busy || (!id && !name.trim())}>{busy ? "正在保存…" : id ? "加入歌单" : "创建并加入"}</button>
    </form>
  </dialog>;
}
