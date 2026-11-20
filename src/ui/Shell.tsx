import { useCallback, useEffect, useRef, useState } from "react";
import { desktop, type Playlist, type Session } from "../bridge";
import { Brand, Icon, IconButton, type IconName } from "./Icons";
import { message, Notice, Reconnect } from "./Connection";
import { playlistPath, useRoute, type Section } from "./navigation";
import { PlayerBar } from "./Player";
import { readPreference, savePreference, themeNames, type Theme } from "./preferences";
import { TrackPage } from "./TrackPage";
import { usePlayer } from "./usePlayer";
import { CategoryBrowser } from "./CategoryBrowser";
import { Recommendations } from "./Recommendations";
import { useWorkTypes, WorkHistory, workPath, Works } from "./Works";

const sections: Record<Section, { name: string; icon: IconName }> = {
  search: { name: "搜索", icon: "search" },
  library: { name: "曲目列表", icon: "library" },
  playlists: { name: "我的歌单", icon: "folder" },
  favorites: { name: "我的收藏", icon: "heart" },
  history: { name: "最近播放", icon: "clock" },
  settings: { name: "设置", icon: "user" },
  recommendations: { name: "推荐", icon: "home" },
  works: { name: "作品库", icon: "album" },
};
type SettingsProps = { session: Session; theme: Theme; setTheme: (value: Theme) => void; onDisconnect: () => void };

function Settings({ session, theme, setTheme, onDisconnect }: SettingsProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function disconnect() {
    setBusy(true);
    setError("");
    try { await desktop.disconnect(); onDisconnect(); }
    catch (reason) {
      const stillConnected = await desktop.session().catch(() => session);
      if (!stillConnected) { onDisconnect(); return; }
      setError(message(reason)); setBusy(false);
    }
  }
  return <div className="settings">
    <section><h2>连接与账号</h2><dl>
      <div><dt>账号</dt><dd>{session.userName}</dd></div>
      <div><dt>服务器</dt><dd>{session.endpoint}</dd></div>
      <div><dt>服务器版本</dt><dd>{session.serverVersion || "未知"}</dd></div>
    </dl>{error && <Notice error={error} />}
      <button disabled={busy} onClick={() => void disconnect()}><Icon name="exit" size={16} />{busy ? "正在断开…" : "退出登录"}</button></section>
    <section><h2>外观</h2><label className="setting-line">界面主题 <select value={theme} onChange={e => setTheme(e.target.value as Theme)}>
      {Object.entries(themeNames).map(([value, name]) => <option key={value} value={value}>{name}</option>)}
    </select></label></section>
  </div>;
}

function Group({ label, children }: { label: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(() => readPreference("group." + label) !== "closed");
  return <div className="nav-group"><button className="group-toggle" aria-expanded={open}
    onClick={() => { setOpen(!open); savePreference("group." + label, open ? "closed" : "open"); }}>
    <span className={open ? "" : "collapsed"}><Icon name="chevron" size={14} /></span>{label}</button>
    {open && children}</div>;
}

function PlaylistList({ playlists, loading, error, reload }: { playlists: Playlist[]; loading: boolean; error: string; reload: () => void }) {
  return <div>
    <div className="section-title"><span>全部歌单</span></div>
    <div className="list-toolbar"><span className="list-count">共 {playlists.length} 个歌单</span><IconButton icon="refresh" label="刷新歌单" disabled={loading} onClick={reload} /></div>
    {error && <Notice error={error} retry={reload} />}
    {loading && <p className="loading" role="status">正在读取歌单…</p>}
    {!error && !loading && !playlists.length && <div className="empty"><Icon name="folder" size={38} /><h3>还没有歌单</h3><p>你的 MusicFS 歌单会显示在这里。</p></div>}
    <div className="playlist-grid">{playlists.map((item, index) => <a className="playlist-tile" href={playlistPath(item.id)} key={item.id}>
      <div className={"playlist-art style-" + index % 4} aria-hidden="true"><i /><Icon name="music" size={25} /></div>
      <strong>{item.name}</strong></a>)}</div>
  </div>;
}

export function Shell(props: SettingsProps) {
  const { session, theme, setTheme } = props;
  const route = useRoute();
  const player = usePlayer();
  const works = useWorkTypes();
  const scope = JSON.stringify([session.endpoint, session.userName]);
  const [worksOpen, setWorksOpen] = useState(() => readPreference("works." + scope) === "open");
  const [historyTypeId, setHistoryTypeId] = useState("");
  const [playlists, setPlaylists] = useState<Playlist[]>([]);
  const [playlistError, setPlaylistError] = useState("");
  const [playlistBusy, setPlaylistBusy] = useState(true);
  const [themeOpen, setThemeOpen] = useState(false);
  const [reconnectError, setReconnectError] = useState("");
  const themeMenu = useRef<HTMLDivElement>(null);
  const themeButton = useRef<HTMLButtonElement>(null);
  const generation = useRef(0);
  const loadPlaylists = useCallback(async () => {
    const request = ++generation.current;
    setPlaylistBusy(true);
    setPlaylistError("");
    try { const data = await desktop.playlists(); if (request === generation.current) setPlaylists(data); }
    catch (reason) { if (request === generation.current) setPlaylistError(message(reason)); }
    finally { if (request === generation.current) setPlaylistBusy(false); }
  }, []);
  useEffect(() => { void loadPlaylists(); return () => { generation.current++; }; }, [loadPlaylists]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k" && !document.querySelector("dialog[open]")) {
        event.preventDefault();
        if (route.section !== "search") window.location.hash = "/search";
        else document.getElementById("track-search")?.focus();
      } else if (event.key === "Escape" && themeOpen) { setThemeOpen(false); themeButton.current?.focus(); }
    };
    const dismiss = (event: PointerEvent) => { if (!themeMenu.current?.contains(event.target as Node)) setThemeOpen(false); };
    window.addEventListener("keydown", key);
    if (themeOpen) document.addEventListener("pointerdown", dismiss);
    return () => { window.removeEventListener("keydown", key); document.removeEventListener("pointerdown", dismiss); };
  }, [route.section, themeOpen]);
  const nav = (key: Section) => <a className="nav-link" href={"#/" + key} title={sections[key].name}
    aria-current={route.section === key && !route.playlistId ? "page" : undefined}><Icon name={sections[key].icon} /><span>{sections[key].name}</span></a>;
  const selected = playlists.find(item => item.id === route.playlistId);
  const workType = works.types.find(type => type.id === route.workTypeId);
  const historyTypes = works.types.filter(type => type.separateHistory);
  const historyType = historyTypes.find(type => type.id === historyTypeId);
  const title = route.workId ? "作品详情" : workType ? workType.name : route.playlistId ? selected?.name || "歌单详情" : sections[route.section].name;
  const backHref = route.workId && workType ? workPath(workType.id) : route.playlistId ? "#/playlists" : "#/library";
  const backLabel = route.workId && workType ? "返回" + workType.name : route.playlistId ? "返回歌单列表" : "返回曲目列表";
  const trackProps = { player, route, playlists, reloadPlaylists: loadPlaylists };
  function content() {
    switch (route.section) {
      case "settings": return <Settings {...props} />;
      case "recommendations": return <Recommendations />;
      case "works": return workType ? <Works type={workType} workId={route.workId} /> : works.loading
        ? <p className="loading">正在读取作品类型…</p>
        : <Notice error={works.error || "未找到此作品类型。"} retry={works.refresh} />;
      case "library": return <CategoryBrowser>{filter => <TrackPage key={filter?.kind + "/" + filter?.id} {...trackProps} categoryFilter={filter} />}</CategoryBrowser>;
      case "playlists":
        if (!route.playlistId) return <PlaylistList playlists={playlists} error={playlistError} loading={playlistBusy} reload={() => void loadPlaylists()} />;
        if (!selected) return playlistBusy ? <p className="loading">正在读取歌单…</p> : <Notice error={playlistError || "未找到此歌单。"} retry={() => void loadPlaylists()} />;
        return <TrackPage key={selected.snapshotRevision} {...trackProps} playlist={selected} />;
      case "history": return <>
        {historyTypes.length > 0 && <nav className="category-tabs" aria-label="播放历史分类">
          <button aria-pressed={!historyType} onClick={() => setHistoryTypeId("")}>音乐</button>
          {historyTypes.map(type => <button key={type.id} aria-pressed={type.id === historyTypeId} onClick={() => setHistoryTypeId(type.id)}>{type.name}</button>)}
        </nav>}
        {historyType ? <WorkHistory type={historyType} /> : <TrackPage {...trackProps} />}
      </>;
      default: return <TrackPage {...trackProps} />;
    }
  }
  async function reconnect() {
    try { await desktop.disconnect(); }
    catch (reason) {
      if (await desktop.session().catch(() => session)) { setReconnectError(message(reason)); return; }
    }
    props.onDisconnect();
  }
  return <Reconnect.Provider value={() => void reconnect()}><div className="shell">
    <aside className="sidebar"><a className="brand-link" href="#/library" aria-label="MusicFS 曲目列表"><Brand /></a>
      <nav className="navigation" aria-label="主导航">
        {nav("search")}{nav("recommendations")}
        <button className={"library-toggle nav-link" + (!worksOpen && ["library","works"].includes(route.section) ? " active" : "")}
          aria-label={worksOpen ? "收起作品库" : "展开作品库"} aria-expanded={worksOpen} aria-controls="work-menu"
          onClick={() => { setWorksOpen(!worksOpen); savePreference("works." + scope, worksOpen ? "closed" : "open"); }}>
          <Icon name="album" /><span>作品库</span><i className={worksOpen ? "" : "collapsed"}><Icon name="chevron" size={15} /></i>
        </button>
        <div className="library-children" id="work-menu" hidden={!worksOpen}>
          {nav("library")}
          {works.types.map(type => <a key={type.id} href={workPath(type.id)} className="nav-link" title={type.name}
            aria-current={route.workTypeId === type.id ? "page" : undefined}>
            <Icon name={type.icon === "radio" || type.icon === "headphones" ? type.icon : "disc"} size={18} /><span>{type.name}</span></a>)}
          {works.error && <button className="nav-retry" onClick={works.refresh}>重新读取作品分类</button>}
        </div>
        {nav("playlists")}
        <Group label="我的">{nav("favorites")}{nav("history")}</Group>
        <Group label="歌单">{playlists.map(item => <a key={item.id} href={playlistPath(item.id)} className="nav-link playlist-link"
          aria-current={route.playlistId === item.id ? "page" : undefined} title={item.name}><Icon name="album" size={18} /><span>{item.name}</span></a>)}
          {playlistBusy && <p className="nav-hint">正在读取歌单…</p>}
          {!playlists.length && !playlistError && !playlistBusy && <p className="nav-hint">歌单列表为空</p>}
          {playlistError && <button className="nav-retry" onClick={() => void loadPlaylists()}>重新读取歌单</button>}
        </Group>
      </nav>
    </aside>
    <main className="main"><header className="header">
      {route.section !== "library" && <a className="icon-button back" aria-label={backLabel} href={backHref}><Icon name="back" /></a>}
      <h1 title={title}>{title}</h1>
      <div className="tools">
        <a className="server-state" href="#/settings" title={session.endpoint}><i /><span>已连接</span></a>
        <div className="theme-control" ref={themeMenu}>
          <button className="icon-button" ref={themeButton} title="界面主题" aria-label="界面主题" aria-expanded={themeOpen}
            onClick={() => setThemeOpen(!themeOpen)}><Icon name="theme" /></button>
          {themeOpen && <div className="theme-menu" aria-label="选择主题">{(Object.keys(themeNames) as Theme[]).map(t =>
            <button key={t} aria-pressed={theme === t} onClick={() => { setTheme(t); setThemeOpen(false); themeButton.current?.focus(); }}>
              <i className={"swatch " + t} />{themeNames[t]}{theme === t && <Icon name="check" size={15} />}</button>)}</div>}
        </div>
        <a className="icon-button user-button" href="#/settings" aria-label="账号与设置" title={"账号与设置 · " + session.userName}><Icon name="user" size={18} /></a>
      </div>
    </header>
      <div className="content" key={[route.section, route.playlistId, route.workTypeId, route.workId, route.query].join("/")}>
        {reconnectError && <Notice error={reconnectError} retry={() => void reconnect()} />}
        {content()}
      </div>
    </main>
    <PlayerBar player={player} />
  </div></Reconnect.Provider>;
}
