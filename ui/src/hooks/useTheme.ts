import { useEffect, useState } from "react";
import type { Theme } from "../lib/types";

/** How long the palette cross-fade runs; matches `data-theme-switching` in v3.css. */
const SWITCH_MS = 340;

function systemTheme(): "dark" | "light" {
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(() => {
    const stored = localStorage.getItem("spud.theme");
    return stored === "dark" || stored === "light" || stored === "system" ? stored : "system";
  });
  const [resolved, setResolved] = useState<"dark" | "light">(() =>
    theme === "system" ? systemTheme() : theme,
  );

  useEffect(() => {
    const apply = () => {
      const next = theme === "system" ? systemTheme() : theme;
      const root = document.documentElement;
      // Arm the cross-fade for one window, then let it go. Keeping the class on
      // would turn every later hover into a 340 ms colour ease.
      const switching = root.dataset.theme && root.dataset.theme !== next;
      if (switching) {
        root.dataset.themeSwitching = "";
        window.setTimeout(() => delete root.dataset.themeSwitching, SWITCH_MS);
      }
      root.dataset.theme = next;
      setResolved(next);
    };
    apply();

    if (theme !== "system") return;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);

  const setTheme = (next: Theme) => {
    localStorage.setItem("spud.theme", next);
    setThemeState(next);
  };

  return { theme, resolved, setTheme };
}
