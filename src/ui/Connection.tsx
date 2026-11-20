import { useState, type FormEvent } from "react";
import { desktop, type Session } from "../bridge";
import { Brand, Icon } from "./Icons";
import { readPreference, savePreference } from "./preferences";

export function message(reason: unknown) { return reason instanceof Error ? reason.message : String(reason); }

export function Notice({ error, retry }: { error: string; retry?: () => void }) {
  return <div className="notice" role="alert"><span>{error}</span>{retry && <button onClick={retry}>重试</button>}</div>;
}

export function Connection({ onConnect, initialError }: { onConnect: (session: Session) => void; initialError: string }) {
  const [endpoint, setEndpoint] = useState(() => readPreference("endpoint") ?? "http://127.0.0.1:8080");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(initialError);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy || !token.trim()) return;
    setBusy(true);
    setError("");
    const credential = token.trim();
    setToken("");
    try {
      const session = await desktop.connect(endpoint.trim(), credential);
      savePreference("endpoint", session.endpoint);
      onConnect(session);
    } catch (reason) { setError(message(reason)); }
    finally { setBusy(false); }
  }
  return <main className="connection-page">
    <div className="connection-art" aria-hidden="true">
      <div className="record-scene"><div className="record-disc"><i /></div><div className="record-sleeve">
        <div className="sleeve-design"><i /><i /><i /></div><Icon name="music" size={28} />
      </div></div>
    </div>
    <section className="connection-panel">
      <Brand />
      <h1>连接音乐库</h1>
      <p>使用 MusicFS Server 的地址和登录令牌，打开你的音乐库。</p>
      <form onSubmit={submit}>
        <label htmlFor="endpoint">服务器地址</label>
        <input id="endpoint" type="url" autoComplete="url" spellCheck={false} placeholder="http://127.0.0.1:8080" required
          value={endpoint} onChange={event => setEndpoint(event.target.value)} disabled={busy} />
        <label htmlFor="token">管理员登录令牌</label>
        <input id="token" type="password" autoComplete="off" spellCheck={false} placeholder="输入登录令牌" required
          value={token} onChange={event => setToken(event.target.value)} disabled={busy} />
        {error && <Notice error={error} />}
        <button className="primary" disabled={busy || !endpoint.trim() || !token.trim()}>
          {busy ? "正在连接…" : "连接并登录"}<Icon name="forward" size={17} />
        </button>
      </form>
    </section>
  </main>;
}
