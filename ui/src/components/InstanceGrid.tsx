import { useState } from "react";
import type { CSSProperties } from "react";
import { BrandMark } from "./BrandMark";
import { Icon } from "./Icon";
import { api } from "../lib/api";
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
}: {
  instance: InstanceSummary;
  game: RunningGame | undefined;
  index: number;
  onOpen: () => void;
  onChanged: () => void;
  onRunningChanged: () => void;
}) {
  const [busy, setBusy] = useState<Busy | null>(null);
  const [error, setError] = useState<string | null>(null);

  const install = async (event: React.MouseEvent) => {
    event.stopPropagation();
    setBusy("install");
    setError(null);
    try {
      await api.instances.install(instance.id);
      onChanged();
    } catch (cause) {
      setError(String(cause));
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
    } catch (cause) {
      setError(String(cause));
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
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const remove = async (event: React.MouseEvent) => {
    event.stopPropagation();
    const runningNote = game ? " The running game is stopped first." : "";
    const confirmed = window.confirm(
      `Delete "${instance.name}" and every file inside its .minecraft folder?${runningNote} This cannot be undone.`,
    );
    if (!confirmed) return;
    setBusy("remove");
    setError(null);
    try {
      await api.instances.remove(instance.id);
      onRunningChanged();
      onChanged();
    } catch (cause) {
      setError(String(cause));
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
}: {
  instances: InstanceSummary[];
  running: RunningMap;
  loading: boolean;
  onCreate: () => void;
  onOpen: (id: string) => void;
  onChanged: () => void;
  onRunningChanged: () => void;
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
        <h3>No instances yet</h3>
        <p style={{ margin: "0 auto 18px", maxWidth: 420 }}>
          Every instance gets its own mods, resource packs and settings. Create one and pick a
          mod loader — Fabric, Quilt, Forge, NeoForge or plain vanilla.
        </p>
        <button type="button" className="btn btn-primary" onClick={onCreate}>
          <Icon name="plus" size={15} /> Create instance
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
        />
      ))}
    </div>
  );
}
