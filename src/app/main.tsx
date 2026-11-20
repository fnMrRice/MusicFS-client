import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { desktop, type Session } from "../bridge";
import { Connection, message } from "../ui/Connection";
import { Brand } from "../ui/Icons";
import { Shell } from "../ui/Shell";
import { useTheme } from "../ui/preferences";
import "../ui/base.css";
import "../ui/pages.css";
import "../ui/player.css";
import "../ui/connection.css";

function App() {
  const [session, setSession] = useState<Session | null | undefined>(undefined);
  const [error, setError] = useState("");
  const [theme, setTheme] = useTheme();
  useEffect(() => {
    let current = true;
    desktop.session().then(value => { if (current) setSession(value); }, reason => {
      if (current) { setError(message(reason)); setSession(null); }
    });
    return () => { current = false; };
  }, []);
  if (session === undefined) return <main className="startup" role="status"><Brand /><span>正在读取连接…</span></main>;
  if (!session) return <Connection initialError={error} onConnect={value => { setError(""); setSession(value); }} />;
  return <Shell session={session} theme={theme} setTheme={setTheme} onDisconnect={() => {
    window.location.hash = "/library";
    setSession(null);
  }} />;
}

createRoot(document.getElementById("app")!).render(<StrictMode><App /></StrictMode>);
