import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { desktop, type Page, type Track } from "../bridge";
import { message, Notice } from "./Connection";
import { Icon, IconButton } from "./Icons";
import { time } from "./Player";
import "./works.css";

export interface WorkType { id: string; name: string; icon: string; separateHistory: boolean }
interface Work {
  workId: string; contentType: string; title: string; description: string;
  tags: string[]; credits: { role: string; name: string }[];
}
interface AudioNode {
  id: string; group: string; directory: string; order: number;
  title: string; artists: string[]; album: string | null; sourceTrackId: string | null;
  durationMs: number | null; coverObjectId: string | null;
}
interface Package { id: string; name: string; kind: string; audio: AudioNode[] }
interface Detail { work: Work; packages: Package[] }
interface HistoryEntry { workId: string; workTitle: string; sourceTrackId: string; trackTitle: string; recordedAt: string }

export const workPath = (type: string, id?: string) => "#/works/" + encodeURIComponent(type) + (id ? "/" + encodeURIComponent(id) : "");

function useResource<T>(fetch: () => Promise<T>) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    setData(null);
    void fetch().then(value => { if (active) setData(value); }, reason => {
      if (active) setError(message(reason));
    }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [fetch, revision]);
  return { data, error, loading, refresh: () => setRevision(v => v + 1) };
}

export function useWorkTypes() {
  const request = useCallback(() => invoke<WorkType[]>("content_types"), []);
  const result = useResource(request);
  return { types: result.data ?? [], error: result.error, refresh: result.refresh, loading: result.loading };
}

function WorkList({ type }: { type: WorkType }) {
  const [draft, setDraft] = useState("");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<Work[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  const seen = useRef(new Set<string>());
  const load = useCallback(async (after: string | null) => {
    const current = ++generation.current;
    setError(""); setBusy(true);
    if (!after) { setCursor(null); setItems([]); seen.current.clear(); }
    try {
      const page = await invoke<Page<Work>>("list_works", { contentType: type.id, query, cursor: after });
      if (current !== generation.current) return;
      if (page.nextCursor && seen.current.has(page.nextCursor)) throw new Error("作品列表分页未向前推进。");
      if (page.nextCursor) seen.current.add(page.nextCursor);
      setItems(v => after ? [...v, ...page.items] : page.items);
      setCursor(page.nextCursor);
    } catch (reason) { if (current === generation.current) setError(message(reason)); }
    finally { if (current === generation.current) setBusy(false); }
  }, [type.id, query]);
  useEffect(() => {
    void load(null);
    return () => { generation.current++; };
  }, [load]);
  return <section className="works-page">
    <form className="work-search" onSubmit={event => { event.preventDefault(); setQuery(draft.trim()); }}>
      <input aria-label="搜索作品" maxLength={256} value={draft} placeholder="搜索作品名、创作者或简介" onChange={e => setDraft(e.target.value)} />
      <button type="submit"><Icon name="search" size={16} />搜索</button>
      <IconButton icon="refresh" label="刷新作品列表" disabled={busy} onClick={() => void load(null)} />
    </form>
    {error && <Notice error={error} retry={() => void load(cursor)} />}
    <div className="work-cards">{items.map(work => <a href={workPath(type.id, work.workId)} key={work.workId}>
      <span className="work-mark"><Icon name={type.icon === "disc" ? "disc" : "album"} size={27} /></span>
      <span><strong>{work.title}</strong><small>{work.credits.map(c => c.name).join(" / ") || work.tags.join(" · ")}</small></span>
      <Icon name="forward" size={14} />
    </a>)}</div>
    {busy && <p className="loading" role="status">正在读取作品…</p>}
    {!busy && !error && !items.length && <div className="empty"><Icon name="album" size={36} /><h3>{query ? "没有匹配的作品" : "作品库为空"}</h3></div>}
    {!busy && !error && cursor && <button onClick={() => void load(cursor)}>加载更多作品</button>}
  </section>;
}

function PackageTracks({ pack }: { pack: Package }) {
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const groups = useMemo(() => {
    const result = new Map<string, AudioNode[]>();
    for (const node of pack.audio) {
      const children = result.get(node.group);
      if (children) children.push(node); else result.set(node.group, [node]);
    }
    for (const nodes of result.values()) nodes.sort((a, b) => a.order - b.order);
    return [...result.entries()];
  }, [pack]);
  const play = async (nodes: AudioNode[], selected?: AudioNode) => {
    const tracks: Track[] = nodes.flatMap(node => node.sourceTrackId ? [{
      id: node.sourceTrackId, sourceOnly: true, sourceTrackId: node.sourceTrackId, durationMs: node.durationMs,
      title: node.title, album: node.album, artists: node.artists, coverObjectId: node.coverObjectId,
    }] : []);
    const index = selected ? tracks.findIndex(t => t.sourceTrackId === selected.sourceTrackId) : 0;
    if (!tracks.length || index < 0) return;
    setBusy(true); setError("");
    try { await desktop.play(tracks, index); } catch (reason) { setError(message(reason)); }
    finally { setBusy(false); }
  };
  return <>
    {error && <Notice error={error} />}
    {!groups.length && <p className="loading">此副本暂无音轨。</p>}
    {groups.map(([id, nodes]) => <section className="work-directory" key={id}>
      <header><Icon name="folder" size={17} /><h3>{nodes[0].directory || pack.name}</h3>
        <button disabled={busy || !nodes.some(n => n.sourceTrackId)} onClick={() => void play(nodes)}><Icon name="play" size={13} />播放此目录</button></header>
      <ol>{nodes.map(node => <li key={node.id}>
        <IconButton icon="play" label={"播放 " + node.title} disabled={busy || !node.sourceTrackId} onClick={() => void play(nodes, node)} />
        <div><strong>{node.title}</strong>{node.artists.length > 0 && <small>{node.artists.join(" / ")}</small>}</div>
        <span>{node.sourceTrackId ? time(node.durationMs) : "暂不可用"}</span>
      </li>)}</ol>
    </section>)}
  </>;
}

function DetailPage({ type, id }: { type: WorkType; id: string }) {
  const request = useCallback(() => invoke<Detail>("work_detail", { workId: id }), [id]);
  const state = useResource(request);
  const [selected, setSelected] = useState("");
  const item = state.data?.work;
  const pack = state.data?.packages.find(p => p.id === selected) ?? state.data?.packages[0];
  return <section className="works-page">
    <a className="work-return" href={workPath(type.id)}><Icon name="back" size={16} />返回{type.name}</a>
    {state.loading && <p className="loading" role="status">正在读取作品…</p>}
    {state.error && <Notice error={state.error} retry={state.refresh} />}
    {item && <><header className="work-title"><h2>{item.title}</h2>
      {item.credits.length > 0 && <p>{item.credits.map(c => c.name).join(" / ")}</p>}
      {item.tags.length > 0 && <small>{item.tags.join(" · ")}</small>}</header>
      {item.description && <p className="work-description">{item.description}</p>}
      {pack ? <><label className="work-copy">本地副本
        <select value={pack.id} onChange={e => setSelected(e.target.value)}>{state.data?.packages.map(p =>
          <option key={p.id} value={p.id}>{p.name} · {p.kind === "cd_dump" ? "CD 抓轨" : "目录"}</option>)}</select></label>
        <PackageTracks key={pack.id} pack={pack} /></> : <p className="loading">此作品尚未关联本地副本。</p>}
    </>}
  </section>;
}

export function Works({ type, workId }: { type: WorkType; workId?: string | null }) {
  return workId ? <DetailPage key={workId} id={workId} type={type} /> : <WorkList key={type.id} type={type} />;
}

export function WorkHistory({ type }: { type: WorkType }) {
  const request = useCallback(() => invoke<HistoryEntry[]>("work_history", { contentType: type.id }), [type.id]);
  const { data, error, loading, refresh } = useResource(request);
  return <section className="works-page">
    <div className="list-toolbar"><span>最近播放的{type.name}</span><IconButton icon="refresh" label="刷新作品历史" disabled={loading} onClick={refresh} /></div>
    {error && <Notice error={error} retry={refresh} />}
    {loading && <p className="loading" role="status">正在读取播放历史…</p>}
    {data && !data.length && <p className="loading">暂无播放记录。</p>}
    <div className="work-cards">{data?.map((entry, i) => <a key={entry.sourceTrackId + i} href={workPath(type.id, entry.workId)}>
      <Icon name="clock" size={21} /><span><strong>{entry.trackTitle || entry.workTitle}</strong><small>{entry.workTitle}</small></span>
      <small>{new Date(entry.recordedAt).toLocaleDateString()}</small>
    </a>)}</div>
  </section>;
}
