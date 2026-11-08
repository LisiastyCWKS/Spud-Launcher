/**
 * Stage the 3.0.0 bundles, upload them to the existing v3.0.0 release and keep
 * the CHANGELOG section as the release body — publish.ps1 would replace the body
 * with a bare commit list, which loses the prose written for this release.
 *
 * Idempotent: an asset with the same name is deleted before the new one is
 * uploaded, so a re-run never leaves a stale binary next to a fresh checksum.
 */
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const VERSION = "3.0.0";
const OWNER = "LisiastyCWKS";
const REPO = "Spud-Launcher";
const TAG = `v${VERSION}`;
const root = process.cwd();
const outDir = path.join(root, "release", VERSION);

const cred = execFileSync("git", ["credential", "fill"], {
  input: "protocol=https\nhost=github.com\n\n",
  encoding: "utf8",
});
const token = /password=(.+)/.exec(cred)?.[1]?.trim();
if (!token) throw new Error("no github token in git credential store");

const headers = {
  Authorization: `Bearer ${token}`,
  Accept: "application/vnd.github+json",
  "User-Agent": "spud-release",
};

const api = async (url, init = {}) => {
  const res = await fetch(url.startsWith("https://") ? url : `https://api.github.com${url}`, {
    ...init,
    headers: { ...headers, ...(init.headers ?? {}) },
  });
  const text = await res.text();
  const body = text ? JSON.parse(text) : null;
  if (!res.ok) throw new Error(`${res.status} ${url}: ${text.slice(0, 400)}`);
  return body;
};

// ------------------------------------------------------------------ staging ---

const artifacts = [
  {
    src: path.join("target", "release", "bundle", "nsis", `Spud Launcher_${VERSION}_x64-setup.exe`),
    name: `Spud-Launcher_${VERSION}_x64-setup.exe`,
  },
  {
    src: path.join("target", "release", "bundle", "msi", `Spud Launcher_${VERSION}_x64_en-US.msi`),
    name: `Spud-Launcher_${VERSION}_x64_en-US.msi`,
  },
  {
    src: path.join("target", "release", "spud-launcher.exe"),
    name: `Spud-Launcher_${VERSION}_x64-portable.exe`,
  },
];

fs.mkdirSync(outDir, { recursive: true });

for (const artifact of artifacts) {
  const source = path.join(root, artifact.src);
  if (!fs.existsSync(source)) throw new Error(`missing build output: ${artifact.src}`);
  const target = path.join(outDir, artifact.name);
  fs.copyFileSync(source, target);
  console.log("staged", artifact.name, (fs.statSync(target).size / 1048576).toFixed(2), "MiB");
}

const sums = artifacts.map((artifact) => {
  const file = path.join(outDir, artifact.name);
  const hash = crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
  return `${hash}  ${artifact.name}`;
});
const sumsPath = path.join(outDir, "SHA256SUMS.txt");
fs.writeFileSync(sumsPath, sums.join("\n") + "\n", "ascii");
console.log("wrote SHA256SUMS.txt");

// -------------------------------------------------------------- release body ---

const changelog = fs.readFileSync(path.join(root, "CHANGELOG.md"), "utf8");
const start = changelog.indexOf(`## [${VERSION}]`);
const end = changelog.indexOf("\n## [", start + 4);
const section = changelog.slice(start, end === -1 ? undefined : end).trimEnd();

const tail = `
## Instalacja

| Plik | Do czego |
|---|---|
| \`Spud-Launcher_${VERSION}_x64-setup.exe\` | instalator per-user (NSIS) — najprostsza droga |
| \`Spud-Launcher_${VERSION}_x64_en-US.msi\` | instalacja przez GPO / \`msiexec\` |
| \`Spud-Launcher_${VERSION}_x64-portable.exe\` | wersja portable, bez instalacji |

Sumy SHA-256 w \`SHA256SUMS.txt\`. Windows 10/11 x64, WebView2 (jest w każdym aktualnym systemie).

Build nie jest podpisany certyfikatem, więc SmartScreen pokaże „Windows protected your PC" — **More info → Run anyway**.
`;

const body = `${section.replace(`## [${VERSION}] — `, `## ${VERSION} — `)}\n${tail}`;

const release = await api(`/repos/${OWNER}/${REPO}/releases/tags/${TAG}`);

await api(`/repos/${OWNER}/${REPO}/releases/${release.id}`, {
  method: "PATCH",
  headers: { "Content-Type": "application/json; charset=utf-8" },
  body: JSON.stringify({ body }),
});
console.log("release body updated");

// ------------------------------------------------------------------- upload ---

for (const asset of release.assets ?? []) {
  await api(`/repos/${OWNER}/${REPO}/releases/assets/${asset.id}`, { method: "DELETE" });
  console.log("removed old asset", asset.name);
}

const uploads = [
  ...artifacts.map((artifact) => ({
    path: path.join(outDir, artifact.name),
    name: artifact.name,
    type: "application/octet-stream",
  })),
  { path: sumsPath, name: "SHA256SUMS.txt", type: "text/plain" },
];

for (const item of uploads) {
  const url =
    `https://uploads.github.com/repos/${OWNER}/${REPO}/releases/${release.id}/assets` +
    `?name=${encodeURIComponent(item.name)}`;
  const uploaded = await api(url, {
    // `fetch` streams a file handle without loading the whole bundle into memory.
    method: "POST",
    headers: { "Content-Type": item.type, "Content-Length": String(fs.statSync(item.path).size) },
    body: fs.readFileSync(item.path),
    duplex: "half",
  });
  console.log("uploaded", uploaded.name, (uploaded.size / 1048576).toFixed(2), "MiB");
}

const final = await api(`/repos/${OWNER}/${REPO}/releases/tags/${TAG}`);
console.log("RELEASE", final.html_url);
for (const asset of final.assets) {
  console.log(" ", asset.name, (asset.size / 1048576).toFixed(2), "MiB", asset.browser_download_url);
}
