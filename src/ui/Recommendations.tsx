import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { message, Notice } from "./Connection";
import { Icon } from "./Icons";
import "./recommendations.css";

interface Playlist { externalId: string; title: string; creator: string | null; trackCount: number | null }
interface Item { playlist: Playlist; reason: string }
interface Source {
  instanceId: string; displayName: string | null; items: Item[]; stale: boolean;
  lastSuccessAt: string | null; error: { code: string } | null;
}
interface Feed { sources: Source[]; historyDepth: number }
interface Selection { instance: string; external: string }
interface Detail {
  playlist: Playlist; complete: boolean; observedAt: string; nextCursor: string | null;
  entries: { position: number; externalTrackId: string; unavailable: boolean;
    track: { title: string; artists: string[]; durationMs: number | null } | null }[];
}

function RecommendationDetail({ selection, back }: { selection: Selection; back: () => void }) {
  const [detail, setDetail] = useState<Detail | null>(null);
  const [error, setError] = useState("");
  const [attempt, retry] = useState(0);
  const [limit, setLimit] = useState(50);
  const [cursor, setCursor] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);
  useEffect(() => {
    let active = true;
    setBusy(true);
    setError("");
    void invoke<Detail>("recommendation_playlist", { ...selection, cursor })
      .then(value => {
        if (!active) return;
        setDetail(old => cursor && old ? { ...value, entries: [...old.entries, ...value.entries] } : value);
      })
      .catch(reason => { if (active) setError(message(reason)); })
      .finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, [selection, cursor, attempt]);
  return <section className="rec-detail">
    <button type="button" onClick={back}><Icon name="back" size={16} />返回推荐</button>
    {error && <Notice error={error} retry={() => retry(value => value + 1)} />}
    {busy && <p className="loading" role="status">正在读取在线歌单…</p>}
    {detail && <>
      <header className="rec-detail-head"><span className="rec-art"><Icon name="album" size={38} /></span>
        <div><h2>{detail.playlist.title}</h2>{detail.playlist.creator && <p>{detail.playlist.creator}</p>}
          <small>{detail.playlist.trackCount == null ? "已读取 " + detail.entries.length + " 首" : detail.playlist.trackCount + " 首"} · {new Date(detail.observedAt).toLocaleDateString()}</small></div>
      </header>
      {!detail.complete && <p className="muted">来源仅返回了部分曲目。</p>}
      {detail.entries.length ? <div className="rec-entries"><h3>曲目</h3>
        <ol>{detail.entries.slice(0, limit).map(entry =>
          <li key={entry.position}><span>{entry.track?.title || (entry.unavailable ? "曲目已失效" : entry.externalTrackId)}</span>
            {entry.track?.artists.length ? <small>{entry.track.artists.join(" / ")}</small> : null}</li>)}</ol>
        {(limit < detail.entries.length || detail.nextCursor) && <button disabled={busy || Boolean(error)} onClick={() => {
          if (limit >= detail.entries.length) setCursor(detail.nextCursor);
          setLimit(n => n + 50);
        }}>显示更多曲目</button>}
      </div> : <p className="loading">此歌单暂无可见曲目。</p>}
    </>}
  </section>;
}

export function Recommendations() {
  const [page, setPage] = useState<Feed | null>(null);
  const [offset, setOffset] = useState(0);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<Selection | null>(null);
  const [retry, setRetry] = useState({ history: 0, refresh: false });
  const generation = useRef(0);
  async function load(history: number, refresh: boolean) {
    const request = ++generation.current;
    setBusy(true);
    setError("");
    setRetry({ history, refresh });
    try {
      const result = await invoke<Feed>("recommendation_feed", { history, refresh });
      if (request === generation.current) { setPage(result); setOffset(history); }
    } catch (reason) {
      if (request === generation.current) setError(message(reason));
    } finally {
      if (request === generation.current) setBusy(false);
    }
  }
  useEffect(() => { void load(0, false); return () => { generation.current++; }; }, []);
  if (selected) return <RecommendationDetail selection={selected} back={() => setSelected(null)} />;
  return <section className="recommendation-feed">
    <div className="rec-actions">
      {offset > 0 && <><span>历史推荐 {offset} / {page?.historyDepth}</span>
        <button disabled={busy} onClick={() => void load(0, false)}>返回最新推荐</button></>}
      {Boolean(page?.historyDepth) && <button disabled={busy || offset >= (page?.historyDepth ?? 0)}
        onClick={() => void load(offset + 1, false)}><Icon name="clock" size={16} />上一期</button>}
      <button disabled={busy} onClick={() => void load(0, true)}><Icon name="refresh" size={16} />刷新推荐</button>
    </div>
    {error && <Notice error={error} retry={() => void load(retry.history, retry.refresh)} />}
    {busy && <p className="loading" role="status">正在读取推荐…</p>}
    {page?.sources.map(source => <section className="rec-source" key={source.instanceId}>
      <h2>{source.displayName ?? "未知来源"}</h2>
      {source.stale && <p className="muted">正在显示上次成功获取的推荐{source.lastSuccessAt ? "（" + new Date(source.lastSuccessAt).toLocaleString() + "）" : ""}。</p>}
      {source.error && <p className="notice" role="status">{source.error.code === "PLATFORM_AUTH_REQUIRED"
        ? "此来源需要登录平台账号。" : "来源暂时不可用（" + source.error.code + "）。"}</p>}
      {source.items.length > 0 ? <div className="rec-grid">{source.items.map(({ playlist, reason }) =>
        <button key={playlist.externalId} className="rec-card" onClick={() => setSelected({ instance: source.instanceId, external: playlist.externalId })}>
          <span className="rec-art" aria-hidden="true"><Icon name="album" size={32} /></span>
          <span className="rec-info"><strong>{playlist.title}</strong>
            {playlist.creator && <small>{playlist.creator}</small>}{playlist.trackCount != null && <small>{playlist.trackCount} 首</small>}
            {reason && <small>{reason}</small>}</span>
        </button>)}</div> : !source.error && <p className="muted">此来源暂无推荐歌单。</p>}
    </section>)}
    {!busy && !error && page?.sources.length === 0 && <div className="empty-state"><Icon name="album" size={32} /><h2>暂无推荐来源</h2>
      <p>连接的音乐库尚未提供推荐歌单。</p></div>}
  </section>;
}
