import { useCallback, useEffect, useState } from "react";
import { accentPalette, normaliseHex, type ThemeMode } from "../lib/color";

const STORAGE_KEY = "spud.accent";

function stored(): string | null {
  try {
    return normaliseHex(localStorage.getItem(STORAGE_KEY));
  } catch {
    return null;
  }
}

/**
 * One picked colour, three CSS variables.
 *
 * Kept in `localStorage` next to the theme: it is a per-machine taste, it must
 * apply on the first paint, and it must survive the backend being unreachable.
 * `null` means the stock spud green straight from `theme.css`.
 */
export function useAccent(mode: ThemeMode) {
  const [accent, setAccentState] = useState<string | null>(stored);

  useEffect(() => {
    const root = document.documentElement.style;
    if (!accent) {
      root.removeProperty("--accent");
      root.removeProperty("--accent-strong");
      root.removeProperty("--accent-ink");
      root.removeProperty("--focus");
      root.removeProperty("--success");
      return;
    }

    const palette = accentPalette(accent, mode);
    root.setProperty("--accent", palette.accent);
    root.setProperty("--accent-strong", palette.strong);
    root.setProperty("--accent-ink", palette.ink);
    // Focus rings and success badges follow the accent — one palette, one voice.
    root.setProperty("--focus", palette.accent);
    root.setProperty("--success", palette.accent);
  }, [accent, mode]);

  const setAccent = useCallback((next: string | null) => {
    const value = next ? normaliseHex(next) : null;
    try {
      if (value) localStorage.setItem(STORAGE_KEY, value);
      else localStorage.removeItem(STORAGE_KEY);
    } catch {
      // Private mode or a locked-down WebView: the colour still applies now.
    }
    setAccentState(value);
  }, []);

  return { accent, setAccent };
}
