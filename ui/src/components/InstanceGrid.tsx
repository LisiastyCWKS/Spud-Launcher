import { useState } from "react";
import type { CSSProperties } from "react";
import { BrandMark } from "./BrandMark";
import { Icon } from "./Icon";
import { api } from "../lib/api";
import { confirmDialog } from "../lib/confirm";
import { t } from "../lib/i18n";
import { initials, loaderLabel, relativeTime } from "../lib/format";
import type { InstanceSummary, RunningGame } from "../lib/types";
import type { RunningMap } from "../hooks/useRunning";

type Busy = "install" | "launch" | "stop" | "remove";

function InstanceCard({
  instance,
  game,
  index,
  onOpen,
  onChanged,
  onRunningChanged,
  onError,
  onNotify,
}: {
  instance: InstanceSummary;
  game: RunningGame | undefined;
  index: number;
  onOpen: () => void;
  onChanged: () => void;
  onRunningChanged: () => void;
  onError?: (cause: unknown) => void;
  onNotify?: (title: string, detail?: string) => void;
}) {
  const [busy, setBusy] = useState<Busy | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fail = (cause: unknown) => {
    setError(String(cause));
    onError?.(cause);
  };

  const install = async (event: React.MouseEvent) => {
    event.stopPropagation();
    setBusy("install");
    setError(null);
    try {
      await api.instances.install(instance.id);
      onChanged();
      onNotify?.(t("toast.installed"), instance.name);
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const launch = async (event: React.MouseEvent) => {
    event.stopPropagation();
    setBusy("launch");
    setError(null);
    try {
      await api.instances.launch(instance.id);
      onRunningChanged();
      onNotify?.(t("toast.launched"), instance.name);
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const stop = async (event: React.MouseEvent) => {
    event.stopPropagation();
    setBusy("stop");
    setError(null);
    try {
      await api.instances.stop(instance.id);
      onRunningChanged();
      onNotify?.(t("toast.stopped"), instance.name);
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const remove = async (event: React.MouseEvent) => {
    event.stopPropagation();
    const runningNote = game ? ` ${t("confirm.delete_running")}` : "";
    const confirmed = await confirmDialog({
      title: t("confirm.delete_title", { name: instance.name }),
      description: `${t("confirm.delete_body")}${runningNote}`,
      confirmLabel: t("common.delete"),
      cancelLabel: t("common.cancel"),
      tone: "danger",
    });
    if (!confirmed) return;
    setBusy("remove");
    setError(null);
    try {
      await api.instances.remove(instance.id);
      onRunningChanged();
      onChanged();
      onNotify?.(t("toast.instance_deleted"), instance.name);
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div
      className="card instance-card"
      style={{ "--i": index } as CSSProperties}
      onClick={onOpen}
      role="button"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") onOpen();
      }}
    >
      <div className="instance-top">
        <div className="instance-icon">
          {instance.icon ? <img src={instance.icon} alt="" /> : initials(instance.name)}
        </div>
        <div style={{ minWidth: 0 }}>
          <div className="instance-name">{instance.name}</div>
          <div className="instance-meta">
            <span className="badge">{instance.mc_version}</span>
            <span className="badge">{loaderLabel(instance.loader, instance.loader_version)}</span>
            {game ? <span className="badge badge-accent">Running</span> : null}
          </div>
        </div>
      </div>

      <div className="instance-status">
        <span className={`dot${instance.ready || game ? " ok" : ""}`} />
        {game
          ? `Running · pid ${game.pid}`
          : instance.ready
            ? "Ready to play"
            : "Not installed"}
        <span className="spacer" />
        played {relativeTime(instance.last_played_at)}
      </div>

      {error ? <div className="error">{error}</div> : null}

      <div className="instance-actions">
        {game ? (
          <button
            type="button"
            className="btn btn-primary"
            onClick={stop}
            disabled={busy !== null}
            title={`Stop the game (pid ${game.pid})`}
          >
            {busy === "stop" ? <span className="spinner" /> : <Icon name="stop" size={15} />}
            Stop
          </button>
        ) : instance.ready ? (
          <button type="button" className="btn btn-primary" onClick={launch} disabled={busy !== null}>
            {busy === "launch" ? <span className="spinner" /> : <Icon name="play" size={15} />}
            Play
          </button>
        ) : (
          <button type="button" className="btn btn-primary" onClick={install} disabled={busy !== null}>
            {busy === "install" ? <span className="spinner" /> : <Icon name="download" size={15} />}
            Install
          </button>
        )}
        <button
          type="button"
          className="btn"
          onClick={install}
          disabled={busy !== null || game !== undefined}
          title={game ? "Stop the game before repairing files" : "Verify and repair files"}
        >
          {busy === "install" ? <span className="spinner" /> : <Icon name="refresh" size={15} />}
          Repair
        </button>
        <button
          type="button"
          className="btn btn-sm btn-danger"
          onClick={remove}
          disabled={busy !== null}
          title="Delete this instance"
          aria-label={`Delete ${instance.name}`}
        >
          {busy === "remove" ? <span className="spinner" /> : <Icon name="trash" size={14} />}
        </button>
      </div>
    </div>
  );
}

export function InstanceGrid({
  instances,
  running,
  loading,
  onCreate,
  onOpen,
  onChanged,
  onRunningChanged,
  onError,
  onNotify,
}: {
  instances: InstanceSummary[];
  running: RunningMap;
  loading: boolean;
  onCreate: () => void;
  onOpen: (id: string) => void;
  onChanged: () => void;
  onRunningChanged: () => void;
  onError?: (cause: unknown) => void;
  onNotify?: (title: string, detail?: string) => void;
}) {
  if (loading) {
    return (
      <div className="instance-grid" aria-busy="true" aria-label="Loading instances">
        {[0, 1, 2].map((index) => (
          <div
            key={index}
            className="card instance-card skeleton-card"
            style={{ "--i": index } as CSSProperties}
          >
            <div className="instance-top">
              <div className="skeleton sk-icon" />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div className="skeleton sk-line" style={{ width: "62%" }} />
                <div className="skeleton sk-line" style={{ width: "38%", marginTop: 8 }} />
              </div>
            </div>
            <div className="skeleton sk-line" style={{ width: "72%" }} />
            <div className="instance-actions" style={{ borderTop: 0, paddingTop: 0 }}>
              <div className="skeleton sk-btn" />
              <div className="skeleton sk-btn" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  if (instances.length === 0) {
    return (
      <div className="empty">
        <BrandMark size={52} className="empty-mark" />
        <h3>{t("instances.empty_title")}</h3>
        <p className="empty-lead">{t("instances.empty_body")}</p>
        <button type="button" className="btn btn-primary" onClick={onCreate}>
          <Icon name="plus" size={15} /> {t("instances.empty_action")}
        </button>
      </div>
    );
  }

  return (
    <div className="instance-grid">
      {instances.map((instance, index) => (
        <InstanceCard
          key={instance.id}
          instance={instance}
          index={index}
          game={running[instance.id]}
          onOpen={() => onOpen(instance.id)}
          onChanged={onChanged}
          onRunningChanged={onRunningChanged}
          onError={onError}
          onNotify={onNotify}
        />
      ))}
    </div>
  );
}
