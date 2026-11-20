import { useCallback, useEffect, useRef, useState } from "react";
import { desktop, type CategoryFilter, type Playlist, type Track } from "../bridge";
import { Icon, IconButton } from "./Icons";
import { message, Notice } from "./Connection";
import { searchPath, type Route } from "./navigation";
import { time } from "./Player";
import type { Player } from "./usePlayer";
import { PlaylistEditor } from "./PlaylistEditor";

function readPage(route: Route, cursor: string | null, revision?: number, category?: CategoryFilter) {
  if (category) return desktop.categoryTracks(category, cursor);
  switch (route.section) {
    case "favorites": return desktop.favorites(cursor);
    case "history": return desktop.history(cursor);
    case "playlists": return desktop.playlist(route.playlistId, cursor, revision);
    default: return desktop.tracks(route.query, cursor);
  }
}

const emptyTitles: Partial<Record<Route["section"], string>> = { library: "音乐库还很安静", favorites: "还没有收藏", history: "还没有播放记录", playlists: "歌单为空", search: "没有找到相关曲目" };
const emptyHints: Partial<Record<Route["section"], string>> = {
  library: "在 MusicFS Server 添加音乐目录并完成扫描后，曲目会出现在这里。",
  favorites: "使用曲目右侧的收藏按钮，将喜欢的音乐加入这里。",
  history: "播放音乐后，在这里找到最近听过的曲目。", search: "试试其他歌名、歌手或专辑。",
};

export function TrackPage({ route, player, playlist, playlists, reloadPlaylists, categoryFilter }: {
  route: Route; player: Player; playlist?: Playlist; playlists: Playlist[]; reloadPlaylists: () => Promise<void>; categoryFilter?: CategoryFilter;
}) {
  const revision = playlist?.snapshotRevision;
  const [adding, setAdding] = useState<Track | null>(null);
  const [rebuilding, setRebuilding] = useState(false);
  const rebuildKey = useRef<{ rev: number; key: string } | null>(null);
  const [items, setItems] = useState<Track[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [loadingAll, setLoadingAll] = useState(false);
  const retryCursor = useRef<string | null>(null);
  const [query, setQuery] = useState(route.query);
  const [saving, setSaving] = useState<string | null>(null);
  const [feedback, setFeedback] = useState("");
  const [saved, setSaved] = useState<Set<string>>(new Set());
  const generation = useRef(0);
  const active = useRef(false);
  const search = route.section === "search";
  const waiting = search && !route.query;
  const favorites = route.section === "favorites";
  const { section, playlistId, query: searchTerm } = route;
  const kind = categoryFilter?.kind;
  const categoryId = categoryFilter?.id;
  const fetchPage = useCallback((after: string | null) => readPage({ section, playlistId, query: searchTerm }, after, revision,
    kind && categoryId !== undefined ? { kind, id: categoryId } : undefined), [section, playlistId, searchTerm, revision, kind, categoryId]);
  const load = useCallback(async (after: string | null) => {
    const request = ++generation.current;
    retryCursor.current = after;
    setError("");
    setLoading(true);
    setLoadingAll(false);
    try {
      const page = await fetchPage(after);
      if (request !== generation.current) return;
      setItems(previous => after ? previous.concat(page.items) : page.items);
      setCursor(page.nextCursor);
    } catch (reason) { if (request === generation.current) setError(message(reason)); }
    finally { if (request === generation.current) setLoading(false); }
  }, [fetchPage]);
  useEffect(() => {
    active.current = true;
    if (!waiting) void load(null);
    return () => { generation.current++; active.current = false; };
  }, [waiting, load]);

  async function playAll() {
    if (loading || loadingAll) return;
    const request = ++generation.current;
    const all = [...items];
    let next = cursor;
    const visited = new Set<string>();
    setLoadingAll(true);
    setError("");
    try {
      while (next) {
        if (visited.has(next)) throw new Error("服务器返回了重复的分页位置，请刷新列表。");
        visited.add(next);
        retryCursor.current = next;
        const page = await fetchPage(next);
        if (request !== generation.current) return;
        all.push(...page.items);
        next = page.nextCursor;
        setItems([...all]);
        setCursor(next);
      }
      const start = all.findIndex(track => track.sourceTrackId);
      if (start >= 0) player.play(all, start);
      else setFeedback("列表中没有可播放的音源。");
    } catch (reason) { if (request === generation.current) setError(message(reason)); }
    finally { if (request === generation.current) setLoadingAll(false); }
  }
  async function toggleFavorite(track: Track) {
    if (saving) return;
    const enable = !favorites && !saved.has(track.id);
    setSaving(track.id);
    setFeedback("");
    try {
      await desktop.favorite(track, enable);
      if (!active.current) return;
      setSaved(previous => {
        const next = new Set(previous);
        if (enable) next.add(track.id); else next.delete(track.id);
        return next;
      });
      if (favorites) {
        setItems(previous => previous.filter(item => item.id !== track.id));
        void load(null);
      }
      setFeedback(enable ? "已加入收藏" : "已取消收藏");
    } catch (reason) { if (active.current) setError(message(reason)); }
    finally { if (active.current) setSaving(null); }
  }
  async function rebuild() {
    if (!playlist || rebuilding) return;
    if (rebuildKey.current?.rev !== playlist.ruleRevision) rebuildKey.current = { rev: playlist.ruleRevision, key: crypto.randomUUID() };
    setRebuilding(true);
    setError("");
    try { await desktop.rebuildPlaylist(playlist, rebuildKey.current.key); await reloadPlaylists(); }
    catch (reason) { if (active.current) setError(message(reason)); }
    finally { if (active.current) setRebuilding(false); }
  }

  const snapshot = player.snapshot;
  const current = snapshot?.index != null ? snapshot.queue[snapshot.index] : null;
  return <section className="track-page">
    {search && <form className="search-form" onSubmit={event => { event.preventDefault(); window.location.hash = searchPath(query); }}>
      <Icon name="search" /><input id="track-search" type="search" aria-label="搜索曲库" autoFocus placeholder="搜索歌曲、歌手或专辑"
        value={query} onChange={e => setQuery(e.target.value)} /><button className="primary" disabled={!query.trim()}>搜索</button>
    </form>}
    {waiting ? <div className="empty"><Icon name="search" size={38} /><h3>搜索你的音乐库</h3><p>输入关键词并按回车。</p></div> : <>
      {section !== "library" && <div className="section-title"><span>{search ? "搜索结果" : "全部曲目"}</span></div>}
      {playlist?.stale && <div className="snapshot-notice"><span>歌单有未应用的更改。</span>
        <button disabled={rebuilding} onClick={() => void rebuild()}>{rebuilding ? "正在更新…" : "更新歌单内容"}</button></div>}
      <div className="list-toolbar"><button className="primary" disabled={loading || loadingAll || (!cursor && !items.some(t => t.sourceTrackId))}
        onClick={() => void playAll()}><Icon name="play" size={14} />{loadingAll ? "正在读取曲目…" : "播放全部"}</button>
        {items.length > 0 && <span className="list-count">{cursor ? "已显示 " : "共 "}{items.length} 首</span>}
        <span className="feedback" role="status">{feedback}</span>
        <IconButton icon="refresh" label="刷新曲目" disabled={loading} onClick={() => void load(null)} /></div>
      {error && <Notice error={error} retry={() => void load(retryCursor.current)} />}
      {items.length > 0 && <div className="track-table-wrap"><table className="track-table">
        <thead><tr><th aria-label="序号">#</th><th scope="col">曲名</th><th scope="col">歌手</th><th scope="col">专辑</th>
          <th scope="col">时长</th><th aria-label="曲目操作" /></tr></thead>
        <tbody>{items.map((track, i) => {
          const isCurrent = !!track.sourceTrackId && current?.sourceTrackId === track.sourceTrackId;
          const isFavorite = favorites || saved.has(track.id);
          return <tr key={`${i}-${track.id}`} className={isCurrent ? "current-track" : undefined}
            onDoubleClick={e => { if (!(e.target as HTMLElement).closest("button")) player.play(items, i); }}>
            <td><button title={track.sourceTrackId ? "播放 " + track.title : "暂无可用音源"} aria-label={"播放 " + track.title} disabled={!track.sourceTrackId}
              className="row-play" onClick={() => player.play(items, i)}>
              <span>{isCurrent ? <Icon name="music" size={15} /> : String(i + 1).padStart(2, "0")}</span><Icon name="play" size={13} />
            </button></td>
            <td title={track.title}><span>{track.title}</span>{!track.sourceTrackId && <small className="unavailable">无音源</small>}</td>
            <td title={track.artists.join(" / ")}>{track.artists.join(" / ") || "—"}</td>
            <td title={track.album ?? undefined}>{track.album || "—"}</td><td className="cell-time">{time(track.durationMs)}</td>
            <td><span className="track-actions"><IconButton icon="heart" label={(isFavorite ? "取消收藏 " : "加入收藏 ") + track.title} className={isFavorite ? "favorited" : ""}
              disabled={saving !== null} onClick={() => void toggleFavorite(track)} /><IconButton icon="add" label={"将 " + track.title + " 加入歌单"}
              disabled={!track.sourceTrackId} onClick={() => setAdding(track)} /></span></td>
          </tr>;
        })}</tbody></table></div>}
      {loading && <div className="loading" role="status">正在加载曲目…</div>}
      {!loading && !items.length && !error && <div className="empty"><Icon name={favorites ? "heart" : route.section === "history" ? "clock" : search ? "search" : "disc"} size={38} />
        <h3>{emptyTitles[route.section]}</h3>{emptyHints[route.section] && <p>{emptyHints[route.section]}</p>}</div>}
      {cursor && !loading && !loadingAll && !error && <button className="more" onClick={() => void load(cursor)}>加载更多曲目</button>}
    </>}
    {adding && <PlaylistEditor track={adding} playlists={playlists} close={() => setAdding(null)} saved={() => {
      setAdding(null); setFeedback("已加入歌单"); void reloadPlaylists();
    }} />}
  </section>;
}
