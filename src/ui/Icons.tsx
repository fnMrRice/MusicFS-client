import type { ButtonHTMLAttributes } from "react";

const symbols = {
  music: "M9 18V5l12-2v13M9 8l12-2 M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0 M21 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0",
  search: "M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0m-2 5 6 6",
  library: "M3 4v16m4-16v16m4-16v16m3-15 4-1 4 15-4 1z",
  album: "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zm12 9a5 5 0 1 1-10 0 5 5 0 0 1 10 0m-5 0h.01",
  disc: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0zm-7 0a2 2 0 1 1-4 0 2 2 0 0 1 4 0M6 10a7 7 0 0 1 4-4m4 12a7 7 0 0 0 4-4",
  folder: "M3 7V4h6l3 3h9v13H3z",
  heart: "M12 21 4 13C-2 6 6 0 12 7c6-7 14-1 8 6z",
  clock: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0zm-9-6v6l4 2",
  queue: "M8 5h13M8 12h13M8 19h13M3 5h.01M3 12h.01M3 19h.01",
  play: "m7 4 13 8-13 8z",
  pause: "M8 5v14m8-14v14",
  stop: "M5 5h14v14H5z",
  previous: "m18 4-11 8 11 8zM4 4v16",
  next: "m6 4 11 8-11 8zM20 4v16",
  volume: "m11 4-6 5H2v6h3l6 5zM15 8a5 5 0 0 1 0 8m3-11a9 9 0 0 1 0 14",
  mute: "m11 4-6 5H2v6h3l6 5zM16 9l5 6m0-6-5 6",
  server: "M3 3h18v7H3zM3 14h18v7H3zM6 6.5h1m-1 11h1",
  user: "M16 8a4 4 0 1 1-8 0 4 4 0 0 1 8 0M3 21v-2a9 6 0 0 1 18 0v2",
  theme: "m8 3-5 3-2 5 5 2v8h12v-8l5-2-2-5-5-3c0 5-8 5-8 0z",
  back: "m15 5-7 7 7 7",
  forward: "m9 5 7 7-7 7",
  chevron: "m5 9 7 7 7-7",
  refresh: "M20 4v6h-6M4 20v-6h6m10-4A8 8 0 0 0 6 6m-2 8a8 8 0 0 0 14 4",
  close: "m5 5 14 14m0-14L5 19",
  check: "m4 12 5 5L20 6",
  clear: "M3 6h18m-15 0 1 15h10l1-15M9 6V3h6v3",
  exit: "M10 3H4v18h6m-1-9h12m-5-5 5 5-5 5",
} as const;

export type IconName = keyof typeof symbols;

export function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
    strokeWidth={name === "pause" ? 3.4 : 1.65} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d={symbols[name]} />
  </svg>;
}

export function IconButton({ icon, label, className = "", ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { icon: IconName; label: string }) {
  return <button type="button" aria-label={label} title={label} className={`icon-button ${className}`} {...props}><Icon name={icon} /></button>;
}

export function Brand() {
  return <span className="brand"><Icon name="music" size={32} /><strong>MusicFS</strong></span>;
}
