import { useEffect, useRef, useState, type ReactNode } from "react";
import { desktop, type Category, type CategoryFilter, type CategoryKind } from "../bridge";
import { Icon, type IconName } from "./Icons";
import { message, Notice } from "./Connection";
import "./category.css";

const labels = { all: "全部", artist: "歌手", album: "专辑", genre: "流派" } as const;
const icons: Record<CategoryKind, IconName> = { artist: "user", album: "album", genre: "music" };

function Categories({ kind, select }: { kind: CategoryKind; select: (entry: Category) => void }) {
  const [items, setItems] = useState<Category[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const epoch = useRef(0);
  async function load(after: string | null) {
    const request = ++epoch.current;
    setBusy(true);
    setError("");
    try {
      const result = await desktop.categories(kind, after);
      if (request !== epoch.current) return;
      if (after && result.nextCursor === after) throw new Error("分类分页未向前推进，请重新打开分类。");
      setItems(previous => after ? previous.concat(result.items) : result.items);
      setCursor(result.nextCursor);
    } catch (reason) {
      if (request === epoch.current) setError(message(reason));
    } finally {
      if (request === epoch.current) setBusy(false);
    }
  }
  useEffect(() => {
    void load(null);
    return () => { epoch.current++; };
  }, [kind]);
  return <>
    <div className="category-grid">{items.map(entry =>
      <button key={entry.id} onClick={() => select(entry)}>
        <span className="category-icon"><Icon name={icons[kind]} size={22} /></span>
        <span><strong>{entry.name || "未知" + labels[kind]}</strong><small>{entry.trackCount} 首</small></span>
      </button>
    )}</div>
    {error && <Notice error={error} retry={() => void load(cursor)} />}
    {busy && <p className="loading" role="status">正在读取{labels[kind]}…</p>}
    {!busy && !error && !items.length && <p className="loading">音乐库中暂无{labels[kind]}。</p>}
    {cursor && !error && <button disabled={busy} onClick={() => void load(cursor)}>加载更多</button>}
  </>;
}

export function CategoryBrowser({ children }: { children: (filter?: CategoryFilter) => ReactNode }) {
  const [kind, setKind] = useState<keyof typeof labels>("all");
  const [selected, setSelected] = useState<Category | null>(null);
  return <>
    <nav className="category-tabs" aria-label="曲库视图">
      {(Object.keys(labels) as (keyof typeof labels)[]).map(key =>
        <button key={key} aria-pressed={kind === key} onClick={() => { setKind(key); setSelected(null); }}>{labels[key]}</button>
      )}
    </nav>
    {kind === "all" ? children() : selected ? <section key={kind + selected.id}>
      <div className="category-trail"><button onClick={() => setSelected(null)}>{labels[kind]}</button>
        <Icon name="forward" size={14} /><strong>{selected.name || "未知" + labels[kind]}</strong></div>
      {children({ kind, id: selected.id })}
    </section> : <Categories key={kind} kind={kind} select={setSelected} />}
  </>;
}
