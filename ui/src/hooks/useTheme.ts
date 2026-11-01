import { useEffect, useState } from "react";
import type { Theme } from "../lib/types";

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
      document.documentElement.dataset.theme = next;
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
