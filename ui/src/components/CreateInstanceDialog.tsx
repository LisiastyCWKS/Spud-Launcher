import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { InstanceConfig, LauncherSettings, LoaderBuild, LoaderKind, VersionSummary } from "../lib/types";
import { Modal } from "./Modal";
import { Icon } from "./Icon";

const LOADERS: Array<{ id: LoaderKind; label: string }> = [
  { id: "vanilla", label: "Vanilla" },
  { id: "fabric", label: "Fabric" },
  { id: "quilt", label: "Quilt" },
  { id: "forge", label: "Forge" },
  { id: "neoforge", label: "NeoForge" },
];

export function CreateInstanceDialog({
  open,
  settings,
  onClose,
  onCreated,
}: {
  open: boolean;
  settings: LauncherSettings | null;
  onClose: () => void;
  onCreated: (instance: InstanceConfig) => void;
}) {
  const [versions, setVersions] = useState<VersionSummary[]>([]);
  const [name, setName] = useState("");
  const [mcVersion, setMcVersion] = useState("");
  const [loader, setLoader] = useState<LoaderKind>("vanilla");
  const [builds, setBuilds] = useState<LoaderBuild[]>([]);
  const [loaderVersion, setLoaderVersion] = useState("");
  const [memory, setMemory] = useState(3072);
  const [loadingVersions, setLoadingVersions] = useState(false);
  const [loadingBuilds, setLoadingBuilds] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    setName("");
    setLoader("vanilla");
    setLoaderVersion("");
    setBuilds([]);
    setMemory(settings?.default_memory_mb ?? 3072);
    setLoadingVersions(true);

    api.versions
      .minecraft(settings?.show_snapshots ?? false)
      .then((list) => {
        setVersions(list);
        if (list.length > 0) setMcVersion(list[0].id);
      })
      .catch((cause) => setError(String(cause)))
      .finally(() => setLoadingVersions(false));
  }, [open, settings?.default_memory_mb, settings?.show_snapshots]);

  useEffect(() => {
    if (!open || loader === "vanilla" || !mcVersion) {
      setBuilds([]);
      setLoaderVersion("");
      return;
    }
    let cancelled = false;
    setLoadingBuilds(true);
    setError(null);
    api.versions
      .loaders(loader, mcVersion)
      .then((list) => {
        if (cancelled) return;
        setBuilds(list);
        const preferred = list.find((entry) => entry.stable) ?? list[0];
        setLoaderVersion(preferred?.version ?? "");
      })
      .catch((cause) => {
        if (!cancelled) setError(String(cause));
      })
      .finally(() => {
        if (!cancelled) setLoadingBuilds(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, loader, mcVersion]);

  if (!open) return null;

  const requiresBuild = loader !== "vanilla";
  const canSubmit =
    !saving &&
    name.trim().length > 0 &&
    mcVersion.length > 0 &&
    (!requiresBuild || loaderVersion.trim().length > 0);

  const submit = async () => {
    if (!canSubmit) return;
    setSaving(true);
    setError(null);
    try {
      const instance = await api.instances.create({
        name: name.trim(),
        mc_version: mcVersion,
        loader,
        loader_version: requiresBuild ? loaderVersion.trim() : null,
        max_memory_mb: memory,
      });
      onCreated(instance);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title="Create instance"
      subtitle="An isolated game directory, its own mods and settings."
      onClose={saving ? () => undefined : onClose}
      width={600}
    >
      <div className="stack">
        <div className="field">
          <label htmlFor="new-name">Name</label>
          <input
            id="new-name"
            className="input"
            value={name}
            placeholder="e.g. Create: Astral"
            onChange={(event) => setName(event.target.value)}
            autoFocus
          />
        </div>

        <div className="form-grid">
          <div className="field">
            <label htmlFor="new-version">Minecraft version</label>
            <select
              id="new-version"
              className="select"
              value={mcVersion}
              disabled={loadingVersions}
              onChange={(event) => setMcVersion(event.target.value)}
            >
              {versions.map((version) => (
                <option key={version.id} value={version.id}>
                  {version.id}
                  {version.kind === "release" ? "" : ` (${version.kind})`}
                </option>
              ))}
            </select>
            {loadingVersions ? <span className="field-hint">Loading Mojang manifest…</span> : null}
          </div>

          <div className="field">
            <label htmlFor="new-memory">Memory (MB)</label>
            <input
              id="new-memory"
              className="input"
              type="number"
              min={512}
              max={65536}
              step={512}
              value={memory}
              onChange={(event) => setMemory(Number(event.target.value))}
            />
          </div>
        </div>

        <div className="field">
          <label>Mod loader</label>
          <div className="chips" style={{ marginBottom: 0 }}>
            {LOADERS.map((entry) => (
              <button
                key={entry.id}
                type="button"
                className={`chip${loader === entry.id ? " active" : ""}`}
                onClick={() => setLoader(entry.id)}
              >
                {entry.label}
              </button>
            ))}
          </div>
        </div>

        {requiresBuild ? (
          <div className="field">
            <label htmlFor="new-loader-version">{LOADERS.find((l) => l.id === loader)?.label} version</label>
            {builds.length > 0 ? (
              <select
                id="new-loader-version"
                className="select"
                value={loaderVersion}
                disabled={loadingBuilds}
                onChange={(event) => setLoaderVersion(event.target.value)}
              >
                {builds.map((build) => (
                  <option key={build.version} value={build.version}>
                    {build.version}
                    {build.stable ? " (recommended)" : ""}
                  </option>
                ))}
              </select>
            ) : (
              <input
                id="new-loader-version"
                className="input"
                value={loaderVersion}
                placeholder={loader === "forge" ? "e.g. 47.2.0" : "Loader build"}
                onChange={(event) => setLoaderVersion(event.target.value)}
              />
            )}
            <span className="field-hint">
              {loadingBuilds
                ? "Querying loader metadata…"
                : builds.length === 0
                  ? "No recommended build for this version — enter one manually."
                  : "Files are installed when you press Install on the instance card."}
            </span>
          </div>
        ) : null}

        {error ? <div className="error">{error}</div> : null}

        <div className="modal-actions">
          <button type="button" className="btn" onClick={onClose} disabled={saving}>
            Cancel
          </button>
          <button type="button" className="btn btn-primary" onClick={submit} disabled={!canSubmit}>
            {saving ? <span className="spinner" /> : <Icon name="plus" size={15} />}
            Create
          </button>
        </div>
      </div>
    </Modal>
  );
}
