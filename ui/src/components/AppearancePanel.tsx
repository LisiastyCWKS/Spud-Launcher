import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { classNames } from "../lib/format";
import { loadBackground } from "../hooks/useAppearance";
import type {
  BackgroundFile,
  Corners,
  Density,
  LauncherSettings,
  ThemePreset,
} from "../lib/types";

interface PresetCard {
  id: ThemePreset;
  label: string;
  note: string;
  /** Background, panel, raised panel, ink — the four chips of the swatch. */
  swatch: [string, string, string, string];
}

const PRESETS: PresetCard[] = [
  {
    id: "spud",
    label: "Spud",
    note: "The stock palette",
    swatch: ["#0c0f0d", "#161b17", "#232a24", "#e9efe9"],
  },
  {
    id: "midnight",
    label: "Midnight",
    note: "Navy, cool and quiet",
    swatch: ["#080b14", "#111726", "#1d263c", "#e6ecf8"],
  },
  {
    id: "forest",
    label: "Forest",
    note: "Moss and bark",
    swatch: ["#0a0f0a", "#131c14", "#1f2d21", "#e8f0e6"],
  },
  {
    id: "dusk",
    label: "Dusk",
    note: "Plum and ember",
    swatch: ["#100b12", "#1b1220", "#2b1d33", "#f2e8f4"],
  },
  {
    id: "paper",
    label: "Paper",
    note: "Warm, for daylight",
    swatch: ["#f6f1e7", "#fffdf8", "#eae2d4", "#1a1611"],
  },
];

const DENSITIES: Array<{ id: Density; label: string }> = [
  { id: "comfortable", label: "Comfortable" },
  { id: "compact", label: "Compact" },
];

const CORNERS: Array<{ id: Corners; label: string }> = [
  { id: "sharp", label: "Sharp" },
  { id: "soft", label: "Soft" },
  { id: "round", label: "Round" },
];

function formatBytes(bytes: number) {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/**
 * Appearance editor: palette, layout and background.
 *
 * Every control writes to the settings draft through `patch`, and the panel
 * lifts the same object to App through `onPreview`, so the launcher repaints
 * while a slider is moving. Persistence is still one explicit save — a user who
 * experiments and walks away keeps the look they had.
 */
export function AppearancePanel({
  settings,
  patch,
  onPreview,
  onError,
}: {
  settings: LauncherSettings;
  patch: (next: Partial<LauncherSettings>) => void;
  onPreview: (next: Partial<LauncherSettings>) => void;
  onError: (message: string) => void;
}) {
  const [images, setImages] = useState<BackgroundFile[]>([]);
  const [scanning, setScanning] = useState(true);
  const [openFolder, setOpenFolder] = useState<string | null>(null);
  // Only the selected image is decoded: a folder of 4K wallpapers would
  // otherwise cost tens of megabytes to render a list of names.
  const [preview, setPreview] = useState<string | null>(null);

  const set = (next: Partial<LauncherSettings>) => {
    patch(next);
    onPreview(next);
  };

  const scan = () => {
    setScanning(true);
    api.appearance
      .backgrounds()
      .then(setImages)
      .catch((cause) => onError(String(cause)))
      .finally(() => setScanning(false));
  };

  useEffect(() => {
    scan();
    // A one-shot scan per mount: the panel is the only thing that changes this
    // folder, and it owns the Rescan button.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    let alive = true;
    if (!settings.background) {
      setPreview(null);
      return;
    }
    void loadBackground(settings.background).then((url) => {
      if (alive) setPreview(url);
    });
    return () => {
      alive = false;
    };
  }, [settings.background]);

  return (
    <div className="stack">
      <div className="field">
        <label>Palette</label>
        <div className="preset-grid">
          {PRESETS.map((preset) => (
            <button
              key={preset.id}
              type="button"
              className={classNames("preset-card", settings.theme_preset === preset.id && "active")}
              onClick={() => set({ theme_preset: preset.id })}
            >
              <span
                className="preset-swatch"
                style={{
                  background: preset.swatch[0],
                  border: "1px solid rgba(127,127,127,0.22)",
                }}
              >
                <span style={{ background: preset.swatch[0] }} />
                <span style={{ background: preset.swatch[1] }} />
                <span style={{ background: preset.swatch[2] }} />
                <span style={{ background: preset.swatch[1] }} />
                <span style={{ background: preset.swatch[3] }} />
              </span>
              <span className="preset-name">{preset.label}</span>
              <span className="preset-note">{preset.note}</span>
            </button>
          ))}
        </div>
        <span className="field-hint">
          The palette is layered over the dark or light theme — switching the theme keeps the
          preset.
        </span>
      </div>

      <div className="field">
        <label>Layout</label>
        <div className="row" style={{ flexWrap: "wrap", gap: 14 }}>
          <div className="chips" style={{ marginBottom: 0 }}>
            {DENSITIES.map((entry) => (
              <button
                key={entry.id}
                type="button"
                className={classNames("chip", settings.density === entry.id && "active")}
                onClick={() => set({ density: entry.id })}
              >
                {entry.label}
              </button>
            ))}
          </div>
          <span className="field-hint" style={{ margin: 0 }}>
            rows
          </span>
          <div className="chips" style={{ marginBottom: 0 }}>
            {CORNERS.map((entry) => (
              <button
                key={entry.id}
                type="button"
                className={classNames("chip", settings.corners === entry.id && "active")}
                onClick={() => set({ corners: entry.id })}
              >
                {entry.label}
              </button>
            ))}
          </div>
          <span className="field-hint" style={{ margin: 0 }}>
            corners
          </span>
        </div>
      </div>

      <div className="field">
        <label>Widths</label>
        <div className="slider-row">
          <span>Sidebar</span>
          <input
            type="range"
            min={200}
            max={320}
            step={4}
            value={settings.sidebar_width}
            aria-label="Sidebar width"
            onChange={(event) => set({ sidebar_width: Number(event.target.value) })}
          />
          <span className="slider-value">{settings.sidebar_width} px</span>
        </div>
        <div className="slider-row">
          <span>Content column</span>
          <input
            type="range"
            min={960}
            max={1800}
            step={20}
            value={settings.content_width}
            aria-label="Content width"
            onChange={(event) => set({ content_width: Number(event.target.value) })}
          />
          <span className="slider-value">{settings.content_width} px</span>
        </div>
        <span className="field-hint">
          Below 1024 px the sidebar collapses to icons either way; the content column stops
          growing at the value set here.
        </span>
      </div>

      <label className="checkbox">
        <input
          type="checkbox"
          checked={settings.reduce_motion}
          onChange={(event) => set({ reduce_motion: event.target.checked })}
        />
        Turn off every animation and transition, even when the system allows motion
      </label>

      <div className="field">
        <label>Background image</label>
        {preview ? (
          <div
            className="background-tile"
            style={{ backgroundImage: `url("${preview}")`, aspectRatio: "21 / 7", cursor: "default" }}
            aria-label="Current background"
          />
        ) : null}
        <div className="background-grid">
          <button
            type="button"
            className={classNames("background-tile none", !settings.background && "active")}
            onClick={() => set({ background: "" })}
          >
            None
          </button>
          {images.map((image) => (
            <button
              key={image.name}
              type="button"
              className={classNames(
                "background-tile",
                settings.background === image.name && "active",
              )}
              title={`${image.name} · ${formatBytes(image.bytes)}`}
              onClick={() => set({ background: image.name })}
            >
              <span className="background-tile-name">{image.name}</span>
            </button>
          ))}
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <button type="button" className="btn btn-sm" onClick={scan} disabled={scanning}>
            {scanning ? <span className="spinner" /> : null}
            Rescan folder
          </button>
          <button
            type="button"
            className="btn btn-sm"
            onClick={() =>
              api.appearance
                .openBackgrounds()
                .then(setOpenFolder)
                .catch((cause) => onError(String(cause)))
            }
          >
            Open backgrounds folder
          </button>
          <span className="field-hint" style={{ margin: 0 }}>
            Drop PNG, JPEG, WebP, GIF, AVIF or BMP files in and press Rescan.
          </span>
        </div>
        {openFolder ? (
          <span className="field-hint">
            <code>{openFolder}</code>
          </span>
        ) : null}

        <div className="slider-row">
          <span>Visibility</span>
          <input
            type="range"
            min={0}
            max={100}
            step={1}
            value={settings.background_opacity}
            aria-label="Background visibility"
            disabled={!settings.background}
            onChange={(event) => set({ background_opacity: Number(event.target.value) })}
          />
          <span className="slider-value">{settings.background_opacity}%</span>
        </div>
        <div className="slider-row">
          <span>Blur</span>
          <input
            type="range"
            min={0}
            max={40}
            step={1}
            value={settings.background_blur}
            aria-label="Background blur"
            disabled={!settings.background}
            onChange={(event) => set({ background_blur: Number(event.target.value) })}
          />
          <span className="slider-value">{settings.background_blur} px</span>
        </div>
        <span className="field-hint">
          The image is tinted with the active theme colour, so text stays readable in both themes.
          Files are read from the launcher data folder only — the launcher never serves arbitrary
          paths.
        </span>
      </div>
    </div>
  );
}
