import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { classNames, copyText } from "../lib/format";
import type { JavaRuntime, LauncherSettings, Theme } from "../lib/types";
import { Icon } from "./Icon";
import { AccentPicker } from "./AccentPicker";
import { StoragePanel } from "./StoragePanel";
import { PluginsPanel } from "./PluginsPanel";
import { AiPanel } from "./AiPanel";
import { SyncPanel } from "./SyncPanel";

const THEMES: Array<{ id: Theme; label: string }> = [
  { id: "system", label: "System" },
  { id: "dark", label: "Dark" },
  { id: "light", label: "Light" },
];

const LOCALES: Array<{ id: string; label: string }> = [
  { id: "", label: "Follow the system" },
  { id: "en-US", label: "English (US)" },
  { id: "pl-PL", label: "Polski" },
  { id: "de-DE", label: "Deutsch" },
  { id: "fr-FR", label: "Français" },
  { id: "es-ES", label: "Español" },
  { id: "ru-RU", label: "Русский" },
  { id: "uk-UA", label: "Українська" },
];

export function SettingsPanel({
  settings,
  theme,
  accent,
  onTheme,
  onAccent,
  onSaved,
}: {
  settings: LauncherSettings | null;
  theme: Theme;
  accent: string | null;
  onTheme: (theme: Theme) => void;
  onAccent: (hex: string | null) => void;
  onSaved: (settings: LauncherSettings) => void;
}) {
  const [draft, setDraft] = useState<LauncherSettings | null>(settings);
  const [runtimes, setRuntimes] = useState<JavaRuntime[]>([]);
  const [root, setRoot] = useState("");
  const [detecting, setDetecting] = useState(false);
  const [installing, setInstalling] = useState<number | null>(null);
  const [javaNote, setJavaNote] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  useEffect(() => {
    api
      .root()
      .then(setRoot)
      .catch(() => setRoot(""));
  }, []);

  const patch = (next: Partial<LauncherSettings>) =>
    setDraft((current) => (current ? { ...current, ...next } : current));

  const detect = useCallback(async () => {
    setDetecting(true);
    setError(null);
    try {
      setRuntimes(await api.java.detect());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setDetecting(false);
    }
  }, []);

  const installJava = async (major: number) => {
    setInstalling(major);
    setError(null);
    setJavaNote(null);
    try {
      const result = await api.java.install(major);
      setJavaNote(
        result.bytes > 0
          ? `Installed Java ${result.major} (${result.version}) — ${(result.bytes / 1024 / 1024).toFixed(0)} MB, ${result.path}`
          : `Java ${result.major} (${result.version}) is already installed at ${result.path}`,
      );
      await detect();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setInstalling(null);
    }
  };

  const save = async () => {
    if (!draft) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const next = await api.settings.save(draft);
      setDraft(next);
      onSaved(next);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2000);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  };

  if (!draft) {
    return (
      <div className="empty">
        <span className="spinner" />
      </div>
    );
  }

  return (
    <div>
      <section className="settings-section">
        <h2 className="settings-title">Appearance</h2>
        <p className="settings-sub">The theme is stored per machine and applied instantly.</p>
        <div className="card settings-card">
          <div className="field">
            <label>Theme</label>
            <div className="chips" style={{ marginBottom: 0 }}>
              {THEMES.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  className={classNames("chip", theme === entry.id && "active")}
                  onClick={() => {
                    onTheme(entry.id);
                    patch({ theme: entry.id });
                  }}
                >
                  {entry.label}
                </button>
              ))}
            </div>
          </div>

          <div className="field">
            <label>Accent colour</label>
            <AccentPicker accent={accent} onChange={onAccent} />
          </div>

          <div className="field">
            <label htmlFor="set-locale">Language</label>
            <select
              id="set-locale"
              className="select"
              value={LOCALES.some((entry) => entry.id === draft.locale) ? draft.locale : ""}
              onChange={(event) => patch({ locale: event.target.value })}
            >
              {LOCALES.map((entry) => (
                <option key={entry.id || "system"} value={entry.id}>
                  {entry.label}
                </option>
              ))}
            </select>
            <span className="field-hint">
              Overrides the language Minecraft itself starts in. Empty follows the OS.
            </span>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Downloads</h2>
        <p className="settings-sub">
          Files are content-addressed in a shared cache; every instance reuses them.
        </p>
        <div className="card settings-card">
          <div className="form-grid">
            <div className="field">
              <label htmlFor="set-concurrency">Parallel downloads</label>
              <input
                id="set-concurrency"
                className="input"
                type="number"
                min={1}
                max={32}
                value={draft.concurrency}
                onChange={(event) => patch({ concurrency: Number(event.target.value) })}
              />
              <span className="field-hint">1–32. Eight saturates most CDNs without tripping limits.</span>
            </div>
            <div className="field">
              <label htmlFor="set-memory">Default memory (MB)</label>
              <input
                id="set-memory"
                className="input"
                type="number"
                min={512}
                max={65536}
                step={512}
                value={draft.default_memory_mb}
                onChange={(event) => patch({ default_memory_mb: Number(event.target.value) })}
              />
              <span className="field-hint">Used by new instances unless changed there.</span>
            </div>
          </div>

          <label className="checkbox">
            <input
              type="checkbox"
              checked={draft.show_snapshots}
              onChange={(event) => patch({ show_snapshots: event.target.checked })}
            />
            Show snapshots and old beta versions in the version list
          </label>
        </div>
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Window</h2>
        <p className="settings-sub">
          How the launcher gets out of the way — and back — around a running game.
        </p>
        <div className="card settings-card">
          <label className="checkbox">
            <input
              type="checkbox"
              checked={draft.close_on_launch}
              onChange={(event) => patch({ close_on_launch: event.target.checked })}
            />
            Step aside once the game starts — the launcher minimises so a fullscreen Minecraft owns
            the display
          </label>

          <label className="checkbox">
            <input
              type="checkbox"
              checked={draft.restore_on_exit}
              onChange={(event) => patch({ restore_on_exit: event.target.checked })}
            />
            Come back when the game exits — the window restores and takes focus, ready for the next
            launch
          </label>

          <div className="field" style={{ marginBottom: 0 }}>
            <label>Fullscreen launcher</label>
            <span className="field-hint">
              <kbd>F11</kbd> toggles a fullscreen launcher, <kbd>Esc</kbd> leaves it. Starting a game
              from fullscreen drops the launcher out first, so the switch into Minecraft never
              flickers between two fullscreen windows.
            </span>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Java</h2>
        <p className="settings-sub">
          Runtimes are discovered in PATH, JAVA_HOME, the registry and the bundled `.spud/java`
          folder. Instances can still override this.
        </p>
        <div className="card settings-card">
          <div className="field">
            <label>Preferred runtime for new instances</label>
            <div className="row" style={{ flexWrap: "wrap" }}>
              <select
                className="select"
                style={{ flex: 1, minWidth: 260 }}
                value={draft.default_java ?? ""}
                onChange={(event) => patch({ default_java: event.target.value || null })}
              >
                <option value="">Auto — newest runtime that fits</option>
                {runtimes.map((runtime) => (
                  <option key={runtime.path} value={runtime.path}>
                    Java {runtime.major} · {runtime.vendor} {runtime.version}
                  </option>
                ))}
              </select>
              <button type="button" className="btn" onClick={detect} disabled={detecting}>
                {detecting ? <span className="spinner" /> : <Icon name="search" size={15} />}
                Detect
              </button>
            </div>
            {runtimes.length === 0 && !detecting ? (
              <span className="field-hint">
                {draft.default_java
                  ? draft.default_java
                  : "Press Detect to scan this machine for Java runtimes."}
              </span>
            ) : null}
          </div>

          <label className="checkbox">
            <input
              type="checkbox"
              checked={draft.auto_install_java}
              onChange={(event) => patch({ auto_install_java: event.target.checked })}
            />
            Download a matching Temurin JRE automatically when nothing installed fits
          </label>

          <div className="field" style={{ marginBottom: 0 }}>
            <label>Managed runtimes</label>
            <div className="row" style={{ flexWrap: "wrap" }}>
              <button
                type="button"
                className="btn"
                disabled={installing !== null}
                onClick={() => void installJava(21)}
              >
                {installing === 21 ? <span className="spinner" /> : <Icon name="download" size={15} />}
                Install Java 21
              </button>
              <button
                type="button"
                className="btn"
                disabled={installing !== null}
                onClick={() => void installJava(8)}
              >
                {installing === 8 ? <span className="spinner" /> : <Icon name="download" size={15} />}
                Install Java 8
              </button>
            </div>
            <span className="field-hint">
              Unpacked to <code>&lt;data&gt;/java/</code>, where Detect and every instance can find it.
              No PATH changes, nothing installed system-wide.
            </span>
            {javaNote ? <span className="field-hint">{javaNote}</span> : null}
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Mod storage</h2>
        <p className="settings-sub">
          Every unique jar is kept once and hard-linked into the instances that use it. Cleaning up
          only touches blobs with no remaining references.
        </p>
        <StoragePanel onError={setError} />
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Plugins</h2>
        <p className="settings-sub">
          Small JavaScript extensions kept in the launcher data folder. They can add panels and
          react to launcher events; nothing is enabled until you say so.
        </p>
        <PluginsPanel onError={setError} />
      </section>

      <section className="settings-section">
        <h2 className="settings-title">AI crash analysis</h2>
        <p className="settings-sub">
          Optional written explanations for crash reports, next to the built-in offline rules. The
          provider is your choice; the API key lives in the OS keyring and no report is sent
          without a per-request confirmation.
        </p>
        {draft ? <AiPanel settings={draft} patch={patch} onError={setError} /> : null}
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Cloud sync</h2>
        <p className="settings-sub">
          Full instance backups — worlds, configs, hand-installed jars — written to a folder you
          control. Restores arrive as a new instance, so two machines never overwrite each other.
        </p>
        {draft ? <SyncPanel settings={draft} patch={patch} onError={setError} /> : null}
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Discord</h2>
        <p className="settings-sub">
          Shows "Playing Spud Launcher" (and the instance) on your Discord profile while the launcher
          runs. Nothing is sent anywhere except your own Discord client over the local pipe.
        </p>
        <div className="card settings-card">
          <label className="checkbox">
            <input
              type="checkbox"
              checked={draft.discord_rpc}
              onChange={(event) => patch({ discord_rpc: event.target.checked })}
            />
            Publish Rich Presence
          </label>

          <div className="field" style={{ marginBottom: 0 }}>
            <label htmlFor="set-discord">Discord application ID</label>
            <input
              id="set-discord"
              className="input mono"
              value={draft.discord_client_id ?? ""}
              placeholder="000000000000000000"
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => patch({ discord_client_id: event.target.value || null })}
            />
            <span className="field-hint">
              discord.com/developers → New Application → copy the Application ID. Without one the
              launcher stays silent on Discord; the toggle alone is not enough.
            </span>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Microsoft sign-in</h2>
        <p className="settings-sub">
          The device-code flow needs an Azure application (client) ID. One registration works for
          every user of this launcher.
        </p>
        <div className="card settings-card">
          <div className="field">
            <label htmlFor="set-ms">Azure application (client) ID</label>
            <input
              id="set-ms"
              className="input mono"
              value={draft.ms_client_id ?? ""}
              placeholder="00000000-0000-0000-0000-000000000000"
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => patch({ ms_client_id: event.target.value || null })}
            />
            <span className="field-hint">
              portal.azure.com → Microsoft Entra ID → App registrations → New registration. Pick
              "Personal Microsoft accounts", leave the redirect URI empty, then enable "Allow public
              client flows". Paste the Application (client) ID here. The{" "}
              <code>SPUD_MS_CLIENT_ID</code> environment variable overrides this value.
            </span>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Endpoints</h2>
        <p className="settings-sub">
          Mirrors and proxies can be pointed elsewhere. Leave a field empty to restore the default.
        </p>
        <div className="card settings-card">
          <div className="field">
            <label htmlFor="set-meta">Mojang version manifest</label>
            <input
              id="set-meta"
              className="input mono"
              value={draft.mojang_meta}
              onChange={(event) => patch({ mojang_meta: event.target.value })}
            />
          </div>
          <div className="field">
            <label htmlFor="set-modrinth">Modrinth API</label>
            <input
              id="set-modrinth"
              className="input mono"
              value={draft.modrinth_api}
              onChange={(event) => patch({ modrinth_api: event.target.value })}
            />
          </div>
          <div className="field">
            <label htmlFor="set-cf">CurseForge API key</label>
            <input
              id="set-cf"
              className="input mono"
              value={draft.curseforge_api_key ?? ""}
              placeholder="optional — required by CurseForge for any request"
              onChange={(event) => patch({ curseforge_api_key: event.target.value || null })}
            />
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2 className="settings-title">Data</h2>
        <p className="settings-sub">
          The launcher writes settings, instances, the shared cache and game logs here. Drop an empty
          `portable.txt` next to the executable to keep everything on a USB stick instead.
        </p>
        <div className="card settings-card">
          <div className="field">
            <label>Data directory</label>
            <div className="row">
              <input className="input mono" value={root} readOnly />
              <button
                type="button"
                className="btn"
                onClick={async () => {
                  await copyText(root);
                  setSaved(true);
                  window.setTimeout(() => setSaved(false), 2000);
                }}
                disabled={!root}
              >
                <Icon name="copy" size={15} /> Copy
              </button>
            </div>
          </div>
        </div>
      </section>

      {error ? <div className="error">{error}</div> : null}

      <div className="form-actions" style={{ maxWidth: 780 }}>
        <button type="button" className="btn btn-primary" onClick={save} disabled={saving}>
          {saving ? <span className="spinner" /> : <Icon name={saved ? "check" : "settings"} size={15} />}
          {saved ? "Saved" : "Save settings"}
        </button>
      </div>
    </div>
  );
}
