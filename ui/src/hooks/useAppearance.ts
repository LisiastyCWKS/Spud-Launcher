import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Corners, Density, ThemePreset } from "../lib/types";

/**
 * Everything the shell needs to repaint itself.
 *
 * Kept as one flat object rather than five props: every field lands in the same
 * effect, and a settings panel edits them together through one draft.
 */
export interface AppearanceSettings {
  preset: ThemePreset;
  density: Density;
  corners: Corners;
  /** Bare file name inside `<root>/backgrounds/`; empty means none. */
  background: string;
  backgroundOpacity: number;
  backgroundBlur: number;
  sidebarWidth: number;
  contentWidth: number;
  reduceMotion: boolean;
}

export const STOCK_APPEARANCE: AppearanceSettings = {
  preset: "spud",
  density: "comfortable",
  corners: "soft",
  background: "",
  backgroundOpacity: 32,
  backgroundBlur: 14,
  sidebarWidth: 236,
  contentWidth: 1240,
  reduceMotion: false,
};

/**
 * Background images are a few megabytes, so a name is resolved once per session.
 * The map is module-level on purpose: switching views unmounts nothing here, but
 * a remount must not re-read the file over IPC.
 */
const imageCache = new Map<string, string | null>();

/** Resolve one background name to a `data:` URL, memoised for the session. */
export function loadBackground(name: string): Promise<string | null> {
  if (!name) return Promise.resolve(null);
  if (imageCache.has(name)) return Promise.resolve(imageCache.get(name) ?? null);
  return api.appearance
    .background(name)
    .then((data) => {
      imageCache.set(name, data);
      return data;
    })
    // A missing file is "no background", not an error banner: the setting
    // survives and starts working again when the image comes back.
    .catch(() => {
      imageCache.set(name, null);
      return null;
    });
}

function useBackgroundUrl(name: string) {
  const [url, setUrl] = useState<string | null>(() => (name ? imageCache.get(name) ?? null : null));

  useEffect(() => {
    if (!name) {
      setUrl(null);
      return;
    }
    if (imageCache.has(name)) {
      setUrl(imageCache.get(name) ?? null);
      return;
    }

    let alive = true;
    void loadBackground(name).then((data) => {
      if (alive) setUrl(data);
    });

    return () => {
      alive = false;
    };
  }, [name]);

  return url;
}

/**
 * Apply the personalization settings as CSS variables and data attributes.
 *
 * Nothing here rebuilds the component tree: presets are `[data-preset]` rules,
 * density and corners are `[data-density]` / `[data-corners]` rules, and the
 * free-form sizes are two variables the layout already reads.
 */
export function useAppearance(settings: AppearanceSettings) {
  const image = useBackgroundUrl(settings.background);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.preset = settings.preset;
    root.dataset.density = settings.density;
    root.dataset.corners = settings.corners;
    root.dataset.motion = settings.reduceMotion ? "off" : "on";
    // `data-bg` is what turns the panels into glass; it follows the resolved
    // image, not the setting, so a deleted file does not leave a see-through
    // shell over nothing.
    if (image) root.dataset.bg = "on";
    else delete root.dataset.bg;

    const style = root.style;
    style.setProperty("--sidebar", `${settings.sidebarWidth}px`);
    style.setProperty("--content-max", `${settings.contentWidth}px`);
    style.setProperty("--bg-opacity", String(settings.backgroundOpacity / 100));
    style.setProperty("--bg-blur", `${settings.backgroundBlur}px`);
    if (image) style.setProperty("--bg-image", `url("${image}")`);
    else style.removeProperty("--bg-image");
  }, [settings, image]);

  return { image };
}
