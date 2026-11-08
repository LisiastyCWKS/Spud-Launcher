import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { t } from "../lib/i18n";
import type { CurseForgeHit, CurseForgeSearch, InstanceSummary } from "../lib/types";
import { classNames } from "../lib/format";
import { Icon } from "./Icon";

type Kind = "mod" | "modpack" | "resourcepack" | "shader";

const KINDS: Array<{ id: Kind; label: string }> = [
  { id: "mod", label: "Mods" },
  { id: "modpack", label: "Modpacks" },
  { id: "resourcepack", label: "Resource packs" },
  { id: "shader", label: "Shaders" },
];

const PAGE_SIZE = 20;

export function CurseForgePanel({
  instances,
  targetId,
  onTargetChange,
  onError,
}: {
  instances: InstanceSummary[];
  targetId: string | null;
  onTargetChange: (id: string | null) => void;
  onError?: (message: string | null) => void;
}) {
  const [kind, setKind] = useState<Kind>("mod");
  const [text, setText] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [offset, setOffset] = useState(0);
  const [result, setResult] = useState<CurseForgeSearch | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const selected = instances.find((entry) => entry.id === targetId) ?? null;
  const effectiveVersion = selected?.mc_version ?? null;
  const effectiveLoader =
    selected && selected.loader !== "vanilla" ? selected.loader : null;

  const search = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await api.curseforge.search({
        text: submitted,
        project_type: kind,
        game_version: effectiveVersion ?? undefined,
        loader: effectiveLoader ?? undefined,
        limit: PAGE_SIZE,
        offset,
      });
      if (!mounted.current) return;
      setResult(response);
      onError?.(null);
    } catch (cause) {
      if (!mounted.current) return;
      const msg = String(cause);
      setError(msg);
      onError?.(msg);
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [submitted, kind, effectiveVersion, effectiveLoader, offset, onError]);

  useEffect(() => {
    void search();
  }, [search]);

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    setOffset(0);
    setSubmitted(text);
  };

  return (
    <div className="modrinth-panel">
      {/* Instance selector */}
      {instances.length > 0 && (
        <div className="field" style={{ marginBottom: 12 }}>
          <select
            className="select"
            value={targetId ?? ""}
            onChange={(e) => onTargetChange(e.target.value || null)}
          >
            <option value="">All versions</option>
            {instances.map((inst) => (
              <option key={inst.id} value={inst.id}>
                {inst.name} · {inst.mc_version}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Kind tabs */}
      <div className="chips" style={{ marginBottom: 10 }}>
        {KINDS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            className={classNames("chip", kind === entry.id && "active")}
            onClick={() => {
              setKind(entry.id);
              setOffset(0);
            }}
          >
            {entry.label}
          </button>
        ))}
      </div>

      {/* Search form */}
      <form className="row" style={{ marginBottom: 12 }} onSubmit={submit}>
        <input
          className="input"
          style={{ flex: 1 }}
          value={text}
          placeholder={`${t("common.search")} on CurseForge…`}
          onChange={(e) => setText(e.target.value)}
        />
        <button type="submit" className="btn" disabled={loading}>
          {loading ? <span className="spinner" /> : <Icon name="search" size={15} />}
          {t("common.search")}
        </button>
      </form>

      {/* Error */}
      {error ? (
        <div className="notice warn" style={{ marginBottom: 12 }}>
          {error.includes("API key") ? (
            <>
              {t("cf.no_key")}{" "}
            </>
          ) : error}
        </div>
      ) : null}

      {/* Results */}
      {result && !loading ? (
        <>
          <div className="mod-list">
            {result.hits.length === 0 ? (
              <div className="empty">No results.</div>
            ) : (
              result.hits.map((hit) => (
                <CurseForgeCard key={hit.id} hit={hit} />
              ))
            )}
          </div>

          {/* Pagination */}
          {(result.total > PAGE_SIZE) && (
            <div className="row" style={{ marginTop: 12, justifyContent: "center", gap: 8 }}>
              <button
                type="button"
                className="btn btn-sm"
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              >
                ← Prev
              </button>
              <span style={{ fontSize: "0.82rem", alignSelf: "center" }}>
                {offset + 1}–{Math.min(offset + PAGE_SIZE, result.total)} of {result.total}
              </span>
              <button
                type="button"
                className="btn btn-sm"
                disabled={offset + PAGE_SIZE >= result.total}
                onClick={() => setOffset(offset + PAGE_SIZE)}
              >
                Next →
              </button>
            </div>
          )}
        </>
      ) : null}

      {loading && !result ? (
        <div className="empty">
          <span className="spinner" />
        </div>
      ) : null}
    </div>
  );
}

function CurseForgeCard({ hit }: { hit: CurseForgeHit }) {
  const downloads =
    hit.downloads >= 1_000_000
      ? `${(hit.downloads / 1_000_000).toFixed(1)}M`
      : hit.downloads >= 1_000
      ? `${(hit.downloads / 1_000).toFixed(0)}k`
      : String(hit.downloads);

  return (
    <div className="mod-card">
      {hit.icon_url ? (
        <img
          className="mod-icon"
          src={hit.icon_url}
          alt=""
          width={40}
          height={40}
          loading="lazy"
          style={{ borderRadius: "var(--radius-sm)" }}
        />
      ) : (
        <div className="mod-icon-placeholder" />
      )}
      <div className="mod-info">
        <div className="mod-name">{hit.name}</div>
        <div className="mod-summary">{hit.summary}</div>
        <div className="mod-meta">
          <span>{t("cf.by", { author: hit.author })}</span>
          <span>·</span>
          <span>
            <Icon name="download" size={12} />
            {" "}{downloads}
          </span>
          {hit.game_versions.slice(0, 3).map((v) => (
            <span key={v} className="badge">{v}</span>
          ))}
        </div>
      </div>
      <div className="mod-actions">
        <a
          href={hit.url}
          target="_blank"
          rel="noreferrer"
          className="btn btn-sm"
          title={t("cf.open_page")}
        >
          <Icon name="external" size={14} />
        </a>
      </div>
    </div>
  );
}
