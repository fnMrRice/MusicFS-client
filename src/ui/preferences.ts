import { useEffect, useState } from "react";

export type Theme = "dark" | "light" | "system";
export const themeNames: Record<Theme, string> = { dark: "深色", light: "浅色", system: "跟随系统" };

export function readPreference(key: string) {
  try { return localStorage.getItem("musicfs.desktop." + key); } catch { return null; }
}

export function savePreference(key: string, value: string) {
  try { localStorage.setItem("musicfs.desktop." + key, value); } catch { /* 存储不可用时，设置仍在当前窗口生效。 */ }
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => {
    const saved = readPreference("theme");
    return saved === "system" || saved === "light" ? saved : "dark";
  });
  useEffect(() => {
    const system = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => { document.documentElement.dataset.theme = theme === "system" ? (system.matches ? "light" : "dark") : theme; };
    apply();
    savePreference("theme", theme);
    system.addEventListener("change", apply);
    return () => system.removeEventListener("change", apply);
  }, [theme]);
  return [theme, setTheme] as const;
}
