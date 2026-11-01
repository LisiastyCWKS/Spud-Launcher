import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { api } from "../lib/api";
import { classNames, loaderLabel } from "../lib/format";
import type { InstanceSummary, ModrinthSearch, SearchHit } from "../lib/types";
import { Icon } from "./Icon";

type Kind = "mod" | "modpack" | "resourcepack" | "shader";

const KINDS: Array<{ id: Kind; label: string }> = [
  { id: "mod", label: "Mods" },
  { id: "modpack", label: "Modpacks" },
  { id: "resourcepack", label: "Resource packs" },
  { id: "shader", label: "Shaders" },
];

const SORTS: Array<{ id: string; label: string }> = [
  { id: "relevance", label: "Relevance" },
  { id: "downloads", label: "Downloads" },
  { id: "follows", label: "Followers" },
  { id: "newest", label: "Newest" },
  { id: "updated", label: "Recently updated" },
];

const PAGE_SIZE = 20;

export function ModrinthPanel({
  instances,
  targetId,
  onTargetChange,
  fixedTarget = false,
  onInstalled,
  onError,
  onPackInstalled,
  gameVersion,
  loader,
}: {
  instances: InstanceSummary[];
  targetId: string | null;
  onTargetChange: (id: string | null) => void;
  fixedTarget?: boolean;
  onInstalled?: () => void | Promise<void>;
  onError?: (message: string | null) => void;
  onPackInstalled?: (instanceId: string) => void;
  gameVersion?: string | null;
  loader?: string | null;
}) {
  const [kind, setKind] = useState<Kind>("mod");
  const [text, setText] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [sort, setSort] = useState("relevance");
  const [offset, setOffset] = useState(0);
  const [result, setResult] = useState<ModrinthSearch | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const selected = instances.find((entry) => entry.id === targetId) ?? null;
  const effectiveVersion = gameVersion ?? selected?.mc_version ?? null;
  const effectiveLoader =
    loader ?? (selected && selected.loader !== "vanilla" ? selected.loader : null);

  const search = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await api.modrinth.search({
        text: submitted,
        project_type: kind,
        game_version:
          kind !== "modpack" && effectiveVersion ? effectiveVersion : undefined,
        loader: kind === "mod" && effectiveLoader ? effectiveLoader : undefined,
        limit: PAGE_SIZE,
        offset,
        index: sort,
      });
      if (!mounted.current) return;
      setResult(response);
    } catch (cause) {
      if (mounted.current) setError(String(cause));
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [submitted, kind, effectiveVersion, effectiveLoader, offset, sort]);

  useEffect(() => {
    void search();
  }, [search]);

  const install = async (hit: SearchHit) => {
    setError(null);
    onError?.(null);

    if (kind === "modpack") {
      setPending(hit.project_id);
      try {
        const versions = await api.modrinth.versions(hit.project_id);
        const version = versions[0];
        if (!version) throw new Error("this modpack has no downloadable version");
        const installed = await api.mods.installPack(version.id);
        onPackInstalled?.(installed.instance.id);
        await onInstalled?.();
      } catch (cause) {
        setError(String(cause));
      } finally {
        setPending(null);
      }
      return;
    }

    if (!targetId) {
      setError("Pick an instance first — content is installed into its game directory.");
      return;
    }

    setPending(hit.project_id);
    try {
      await api.mods.install(targetId, hit.project_id);
      await onInstalled?.();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPending(null);
    }
  };

  const total = result?.total_hits ?? 0;
  const page = Math.floor(offset / PAGE_SIZE) + 1;
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <div>
      <form
        className="searchbar"
        onSubmit={(event) => {
          event.preventDefault();
          setOffset(0);
          setSubmitted(text.trim());
        }}
      >
        <input
          className="input"
          value={text}
          placeholder={
            kind === "modpack"
              ? "Search modpacks on Modrinth…"
              : kind === "resourcepack"
                ? "Search resource packs…"
                : kind === "shader"
                  ? "Search shaders…"
                  : "Search mods on Modrinth…"
          }
          onChange={(event) => setText(event.target.value)}
          aria-label="Search Modrinth"
        />
        <button type="submit" className="btn btn-primary" disabled={loading}>
          {loading ? <span className="spinner" /> : <Icon name="search" size={15} />}
          Search
        </button>
      </form>

      <div className="row-between" style={{ marginBottom: 12, flexWrap: "wrap", gap: 8 }}>
        <div className="chips" style={{ marginBottom: 0 }}>
          {KINDS.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={classNames("chip", kind === entry.id && "active")}
              onClick={() => {
                setKind(entry.id);
                setOffset(0);
                setResult(null);
              }}
            >
              {entry.label}
            </button>
          ))}
        </div>

        <div className="row" style={{ gap: 8 }}>
          <select
            className="select"
            style={{ width: 170 }}
            value={sort}
            onChange={(event) => {
              setSort(event.target.value);
              setOffset(0);
            }}
            aria-label="Sort results"
          >
            {SORTS.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {!fixedTarget ? (
        <div className="row" style={{ marginBottom: 14, flexWrap: "wrap" }}>
          <label className="muted" style={{ fontSize: 12 }} htmlFor="modrinth-target">
            Install into
          </label>
          <select
            id="modrinth-target"
            className="select"
            style={{ width: 260 }}
            value={targetId ?? ""}
            onChange={(event) => onTargetChange(event.target.value || null)}
          >
            <option value="">
              {kind === "modpack" ? "New instance from the pack" : "No instance selected"}
            </option>
            {instances.map((instance) => (
              <option key={instance.id} value={instance.id}>
                {instance.name} — {instance.mc_version} ·{" "}
                {loaderLabel(instance.loader, instance.loader_version)}
              </option>
            ))}
          </select>
          {effectiveVersion ? (
            <span className="badge">
              filtering {effectiveVersion}
              {effectiveLoader ? ` / ${effectiveLoader}` : ""}
            </span>
          ) : null}
        </div>
      ) : null}

      {error ? <div className="error" style={{ marginBottom: 12 }}>{error}</div> : null}

      {result && result.hits.length === 0 && !loading ? (
        <div className="empty">
          <h3>Nothing matched</h3>
          <p style={{ margin: 0 }}>Try a different name, or clear the instance filter.</p>
        </div>
      ) : null}

      <div className="hit-grid">
        {(result?.hits ?? []).map((hit, index) => (
          <div
            key={hit.project_id}
            className="card hit"
            style={{ "--i": index } as CSSProperties}
          >
            <div className="hit-top">
              <div className="hit-icon">
                {hit.icon_url ? <img src={hit.icon_url} alt="" loading="lazy" /> : <Icon name="package" />}
              </div>
              <div style={{ minWidth: 0 }}>
                <h4 className="hit-title">{hit.title}</h4>
                <div className="hit-author">
                  by {hit.author || "unknown"}
                  {hit.license ? ` · ${hit.license}` : ""}
                </div>
              </div>
            </div>

            <div className="hit-desc">{hit.description}</div>

            <div className="hit-foot">
              <span className="hit-stat">
                <Icon name="download" size={13} /> {compactNumber(hit.downloads)}
              </span>
              <span className="hit-stat">
                <Icon name="compass" size={13} /> {hit.display_categories.slice(0, 2).join(", ") || "—"}
              </span>
              <span className="spacer" />
              <button
                type="button"
                className="btn btn-sm btn-primary"
                onClick={() => install(hit)}
                disabled={pending !== null || (kind !== "modpack" && !targetId)}
                title={
                  kind === "modpack"
                    ? "Create an instance from this pack"
                    : targetId
                      ? "Install with dependencies"
                      : "Pick an instance first"
                }
              >
                {pending === hit.project_id ? <span className="spinner" /> : <Icon name="download" size={14} />}
                {kind === "modpack" ? "Install pack" : "Install"}
              </button>
            </div>
          </div>
        ))}
      </div>

      {loading && !result ? (
        <div className="empty" style={{ marginTop: 12 }}>
          <span className="spinner" style={{ display: "inline-block" }} />
        </div>
      ) : null}

      {total > PAGE_SIZE ? (
        <div className="row-between" style={{ marginTop: 16 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            Page {page} of {pages} · {total.toLocaleString()} results
          </span>
          <div className="row" style={{ gap: 8 }}>
            <button
              type="button"
              className="btn btn-sm"
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              disabled={offset === 0 || loading}
            >
              Previous
            </button>
            <button
              type="button"
              className="btn btn-sm"
              onClick={() => setOffset(offset + PAGE_SIZE)}
              disabled={offset + PAGE_SIZE >= total || loading}
            >
              Next
            </button>
          </div>
        </div>
      ) : null}

    </div>
  );
}

function compactNumber(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0";
  if (value < 1000) return String(value);
  if (value < 1_000_000) return `${(value / 1000).toFixed(value < 10_000 ? 1 : 0)}k`;
  return `${(value / 1_000_000).toFixed(1)}M`;
}
