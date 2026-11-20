import { invoke } from "@tauri-apps/api/core";

export interface Session {
  endpoint: string;
  userName: string;
  serverVersion: string;
}

export interface Track {
  id: string;
  title: string;
  artists: string[];
  album: string | null;
  sourceTrackId: string | null;
  durationMs: number | null;
  coverObjectId?: string | null;
  forcedSourceTrackId?: string | null;
}

export interface Playlist {
  id: string;
  name: string;
  snapshotRevision: number;
}

export interface Page<T> {
  items: T[];
  nextCursor: string | null;
}

export interface PlayerSnapshot {
  queue: Track[];
  index: number | null;
  status: "idle" | "loading" | "playing" | "paused" | "ended" | "error";
  positionMs: number;
  durationMs: number | null;
  volume: number;
  seekable: boolean;
  error: string | null;
}

type Action = "pause" | "resume" | "play" | "previous" | "next" | "stop" | "clear";

const tracks = (collection: "library" | "favorites" | "history", query: string, cursor: string | null) =>
  invoke<Page<Track>>("list_tracks", { collection, query, cursor });
const command = (value: { type: string; index?: number; volume?: number; positionMs?: number }) =>
  invoke<void>("player_command", { command: value });

export const desktop = {
  session: () => invoke<Session | null>("current_session"),
  connect: (endpoint: string, token: string) => invoke<Session>("connect", { endpoint, token }),
  disconnect: () => invoke<void>("disconnect"),
  tracks: (query = "", cursor: string | null = null) => tracks("library", query, cursor),
  favorites: (cursor: string | null = null) => tracks("favorites", "", cursor),
  history: (cursor: string | null = null) => tracks("history", "", cursor),
  favorite: (track: Track, enabled: boolean) => invoke<void>("set_favorite", { id: track.id, enabled }),
  playlists: async () => {
    const items: Playlist[] = [];
    let cursor: string | null = null;
    const seen = new Set<string>();
    do {
      const page: Page<Playlist> = await invoke("list_playlists", { cursor });
      items.push(...page.items);
      cursor = page.nextCursor;
      if (cursor && seen.has(cursor)) throw new Error("Invalid playlist cursor");
      if (cursor) seen.add(cursor);
    } while (cursor);
    return items;
  },
  playlist: (id: string, cursor: string | null = null, revision?: number) =>
    invoke<Page<Track>>("playlist_tracks", { id, cursor, revision }),
  cover: (id: string) => invoke<string>("cover", { id }),
  snapshot: () => invoke<PlayerSnapshot>("player_snapshot"),
  play: (tracks: Track[], index: number) => invoke<void>("play_tracks", { tracks, index }),
  control: (action: Action) => command({ type: action === "resume" ? "play" : action }),
  seek: (positionMs: number) => command({ type: "seek", positionMs: Math.round(positionMs) }),
  volume: (volume: number) => command({ type: "volume", volume }),
  select: (index: number) => command({ type: "select", index }),
};
