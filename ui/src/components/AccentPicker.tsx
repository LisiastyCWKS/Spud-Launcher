import { useEffect, useState } from "react";
import {
  ACCENT_PRESETS,
  DEFAULT_ACCENT,
  hexToHsl,
  hslToHex,
  normaliseHex,
} from "../lib/color";
import { classNames } from "../lib/format";

/**
 * Accent editor: presets, a native colour picker, an RGB hex box and the three
 * HSL sliders. Everything writes straight to the hook, so the whole launcher
 * repaints while the handle is still moving.
 */
export function AccentPicker({
  accent,
  onChange,
}: {
  accent: string | null;
  onChange: (hex: string | null) => void;
}) {
  const current = accent ?? DEFAULT_ACCENT;
  const hsl = hexToHsl(current);
  const [hexDraft, setHexDraft] = useState(current);

  useEffect(() => {
    setHexDraft(current);
  }, [current]);

  const commitHex = () => {
    const next = normaliseHex(hexDraft);
    if (next) onChange(next);
    else setHexDraft(current);
  };

  const setChannel = (key: "h" | "s" | "l", value: number) => {
    onChange(hslToHex({ ...hsl, [key]: value }));
  };

  return (
    <div className="accent-editor">
      <div className="accent-preview">
        <div className="accent-preview-row">
          <span className="accent-preview-mark">s</span>
          <span className="accent-preview-name">Spud Launcher</span>
          <button type="button" className="btn btn-primary btn-sm accent-preview-btn" tabIndex={-1}>
            Play
          </button>
          <span className="badge badge-accent">Ready</span>
        </div>
        <div className="accent-preview-row">
          <span className="accent-preview-link">Browse Modrinth</span>
          <span className="accent-preview-bar">
            <span />
          </span>
        </div>
      </div>

      <div className="chips accent-presets">
        {ACCENT_PRESETS.map((preset) => (
          <button
            key={preset.hex}
            type="button"
            className={classNames(
              "accent-swatch",
              normaliseHex(preset.hex) === normaliseHex(current) && "active",
            )}
            style={{ background: preset.hex }}
            title={preset.label}
            aria-label={preset.label}
            onClick={() => onChange(preset.hex)}
          />
        ))}
        <label className="accent-custom" title="Pick any colour">
          <input
            type="color"
            value={current}
            aria-label="Custom accent colour"
            onChange={(event) => onChange(event.target.value)}
          />
        </label>
      </div>

      <div className="row accent-hex-row">
        <div className="field" style={{ marginBottom: 0, width: 132 }}>
          <label htmlFor="accent-hex">Hex</label>
          <input
            id="accent-hex"
            className="input mono"
            value={hexDraft}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => setHexDraft(event.target.value)}
            onBlur={commitHex}
            onKeyDown={(event) => {
              if (event.key === "Enter") commitHex();
            }}
          />
        </div>
        <div className="field accent-sliders">
          <label htmlFor="accent-hue">Hue · {Math.round(hsl.h)}°</label>
          <input
            id="accent-hue"
            type="range"
            min={0}
            max={360}
            value={Math.round(hsl.h)}
            onChange={(event) => setChannel("h", Number(event.target.value))}
          />
          <label htmlFor="accent-sat">Saturation · {Math.round(hsl.s)}%</label>
          <input
            id="accent-sat"
            type="range"
            min={0}
            max={100}
            value={Math.round(hsl.s)}
            onChange={(event) => setChannel("s", Number(event.target.value))}
          />
          <label htmlFor="accent-light">Brightness · {Math.round(hsl.l)}%</label>
          <input
            id="accent-light"
            type="range"
            min={0}
            max={100}
            value={Math.round(hsl.l)}
            onChange={(event) => setChannel("l", Number(event.target.value))}
          />
        </div>
      </div>

      <div className="row accent-reset-row">
        <button
          type="button"
          className="btn btn-sm"
          onClick={() => onChange(null)}
          disabled={!accent}
        >
          Reset to spud green
        </button>
        <span className="field-hint" style={{ margin: 0 }}>
          One hue drives buttons, links, focus rings and the download bar. Brightness is clamped
          per theme so text stays readable.
        </span>
      </div>
    </div>
  );
}
