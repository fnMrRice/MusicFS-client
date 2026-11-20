import { useEffect, useState } from "react";

const routes = ["library", "favorites", "history", "search", "playlists", "settings"] as const;
export type Section = typeof routes[number];
export interface Route { section: Section; playlistId: string; query: string }

export function useRoute(): Route {
  const [hash, setHash] = useState(window.location.hash);
  useEffect(() => {
    const onNavigate = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onNavigate);
    return () => window.removeEventListener("hashchange", onNavigate);
  }, []);
  const [path, search] = hash.replace(/^#\/?/, "").split("?");
  const [section, encodedId = ""] = path.split("/");
  let playlistId = "";
  if (section === "playlists") {
    try { playlistId = decodeURIComponent(encodedId); } catch { /* 无效的路径回到歌单列表。 */ }
  }
  return {
    section: routes.find(route => route === section) ?? "library",
    playlistId,
    query: section === "search" ? new URLSearchParams(search).get("q")?.trim() || "" : "",
  };
}

export function playlistPath(id: string) { return "#/playlists/" + encodeURIComponent(id); }
export function searchPath(query: string) { return "#/search" + (query.trim() ? "?" + new URLSearchParams({ q: query.trim() }) : ""); }
