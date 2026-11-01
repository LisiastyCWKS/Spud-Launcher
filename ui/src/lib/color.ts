/**
 * Accent colour maths.
 *
 * The whole palette hangs off one hue: the user picks a colour, and the launcher
 * derives the strong/hover shade, the ink that sits on top of it and the focus
 * ring from the same HSL triplet. Clamps keep a deliberately dark pick readable
 * in the dark theme and a pastel readable in the light one.
 */

export type ThemeMode = "dark" | "light";

export interface AccentPalette {
  accent: string;
  strong: string;
  ink: string;
}

export interface Hsl {
  h: number;
  s: number;
  l: number;
}

/** The stock spud green — what the launcher looks like with no pick. */
export const DEFAULT_ACCENT = "#7ee2a8";

export const ACCENT_PRESETS: Array<{ hex: string; label: string }> = [
  { hex: "#7ee2a8", label: "Spud" },
  { hex: "#9ee66a", label: "Lime" },
  { hex: "#5fd6c3", label: "Teal" },
  { hex: "#6fc4ff", label: "Sky" },
  { hex: "#a68bff", label: "Violet" },
  { hex: "#ff8fb1", label: "Rose" },
  { hex: "#ffb65c", label: "Amber" },
  { hex: "#e0e0e0", label: "Graphite" },
];

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

export function isHex(value: string | null | undefined): value is string {
  return typeof value === "string" && /^#?[0-9a-f]{6}$/i.test(value.trim());
}

/** `7ee2a8`, `#7EE2A8` → `#7ee2a8`. Returns null for anything else. */
export function normaliseHex(value: string | null | undefined): string | null {
  if (!isHex(value)) return null;
  const body = value.trim().replace(/^#/, "").toLowerCase();
  return `#${body}`;
}

export function hexToHsl(hex: string): Hsl {
  const body = (normaliseHex(hex) ?? DEFAULT_ACCENT).slice(1);
  const r = parseInt(body.slice(0, 2), 16) / 255;
  const g = parseInt(body.slice(2, 4), 16) / 255;
  const b = parseInt(body.slice(4, 6), 16) / 255;

  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;

  if (max === min) return { h: 0, s: 0, l: l * 100 };

  const delta = max - min;
  const s = l > 0.5 ? delta / (2 - max - min) : delta / (max + min);

  let h: number;
  if (max === r) h = ((g - b) / delta + (g < b ? 6 : 0)) / 6;
  else if (max === g) h = ((b - r) / delta + 2) / 6;
  else h = ((r - g) / delta + 4) / 6;

  return { h: h * 360, s: s * 100, l: l * 100 };
}

export function hslToHex({ h, s, l }: Hsl): string {
  const hue = ((h % 360) + 360) % 360;
  const sat = clamp(s, 0, 100) / 100;
  const light = clamp(l, 0, 100) / 100;

  const channel = (n: number) => {
    const k = (n + hue / 30) % 12;
    const a = sat * Math.min(light, 1 - light);
    const value = light - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(value * 255)
      .toString(16)
      .padStart(2, "0");
  };

  return `#${channel(0)}${channel(8)}${channel(4)}`;
}

function relativeLuminance(hex: string): number {
  const body = (normaliseHex(hex) ?? DEFAULT_ACCENT).slice(1);
  const channels = [0, 2, 4].map((offset) => {
    const value = parseInt(body.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

/**
 * Turn a picked colour into the three values the stylesheet needs.
 *
 * Dark theme: the accent is lifted to 52–74% lightness so it reads as text and
 * glows nicely on the near-black surfaces. Light theme: pushed down to 28–40%
 * so it passes as body text on white. `ink` is whatever sits on the accent
 * itself — buttons, badges, the brand mark.
 */
export function accentPalette(base: string, mode: ThemeMode): AccentPalette {
  const { h, s, l } = hexToHsl(base);
  const saturation = clamp(s, 22, 96);

  const accentL = mode === "dark" ? clamp(l, 52, 74) : clamp(l, 26, 40);
  const strongL =
    mode === "dark"
      ? clamp(accentL - 15, 32, 62)
      : clamp(accentL - 9, 14, 34);

  const accent = hslToHex({ h, s: saturation, l: accentL });
  const strong = hslToHex({ h, s: saturation, l: strongL });
  // Pick the ink by WCAG contrast, not by a lightness guess: a mid pink reads
  // far better with near-black on top than with white.
  const luminance = relativeLuminance(accent);
  const onWhite = 1.05 / (luminance + 0.05);
  const onInk = (luminance + 0.05) / 0.056;
  const ink = onInk >= onWhite ? "#06130b" : "#ffffff";

  return { accent, strong, ink };
}
