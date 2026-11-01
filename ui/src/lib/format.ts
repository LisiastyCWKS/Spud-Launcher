import type { LoaderKind } from "./types";

export function bytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  const amount = value / 1024 ** exponent;
  return `${amount >= 100 ? amount.toFixed(0) : amount.toFixed(1)} ${units[exponent]}`;
}

export function relativeTime(iso: string | null): string {
  if (!iso) return "never";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "never";
  const diff = Math.max(0, Date.now() - then);
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} d ago`;
  return new Date(iso).toLocaleDateString();
}

export function loaderLabel(loader: LoaderKind, version: string | null): string {
  const name =
    loader === "vanilla"
      ? "Vanilla"
      : loader === "neoforge"
        ? "NeoForge"
        : loader.charAt(0).toUpperCase() + loader.slice(1);
  return version ? `${name} ${version}` : name;
}

export function initials(name: string): string {
  const clean = name.trim();
  if (!clean) return "?";
  const parts = clean.split(/\s+/).slice(0, 2);
  return parts.map((part) => part[0]?.toUpperCase() ?? "").join("");
}

export function copyText(text: string): Promise<void> {
  return navigator.clipboard.writeText(text);
}

export function classNames(...values: Array<string | false | null | undefined>): string {
  return values.filter(Boolean).join(" ");
}
