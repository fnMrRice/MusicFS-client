import { useEffect, useState } from "react";

const routes = ["library", "favorites", "history", "search", "playlists", "settings", "recommendations", "works"] as const;
export type Section = typeof routes[number];
export interface Route { section: Section; playlistId: string; query: string; workTypeId?: string; workId?: string }

function decode(segment: string | undefined) {
  try { return decodeURIComponent(segment || ""); } catch { return ""; }
}

export function useRoute(): Route {
  const [hash, setHash] = useState(window.location.hash);
  useEffect(() => {
    const onNavigate = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onNavigate);
    return () => window.removeEventListener("hashchange", onNavigate);
  }, []);
  const [path, search] = hash.replace(/^#\/?/, "").split("?");
  const [section, id, entry] = path.split("/");
  return {
    section: routes.find(route => route === section) ?? "library",
    playlistId: section === "playlists" ? decode(id) : "",
    workTypeId: section === "works" ? decode(id) : undefined,
    workId: section === "works" && entry ? decode(entry) : undefined,
    query: section === "search" ? new URLSearchParams(search).get("q")?.trim() || "" : "",
  };
}

export function playlistPath(id: string) { return "#/playlists/" + encodeURIComponent(id); }
export function searchPath(query: string) { return "#/search" + (query.trim() ? "?" + new URLSearchParams({ q: query.trim() }) : ""); }
