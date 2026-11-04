import { pluginPanels, pluginSnapshot, subscribePlugins } from "../lib/plugins";
import type { PluginPanelDefinition, PluginRuntime } from "../lib/plugins";
import { Icon } from "./Icon";
import { useEffect, useRef, useState } from "react";

/** Live view of the plugin runtime, kept in sync by the host's own store. */
export function usePluginRuntime(): PluginRuntime[] {
  const [runtimes, setRuntimes] = useState<PluginRuntime[]>(pluginSnapshot);

  useEffect(() => {
    setRuntimes(pluginSnapshot());
    // The host is loaded once by App on start-up; this view only subscribes, so
    // navigating back and forth never re-runs a plugin's entry point.
    return subscribePlugins(() => setRuntimes(pluginSnapshot()));
  }, []);

  return runtimes;
}

/**
 * One plugin panel. The plugin gets a bare element and owns its subtree; React
 * never diffs inside it, so a plugin cannot corrupt the launcher's tree by
 * mutating DOM React believes it controls.
 */
function PanelSlot({ definition }: { definition: PluginPanelDefinition }) {
  const host = useRef<HTMLDivElement | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    element.innerHTML = "";
    setFailure(null);

    let cleanup: void | (() => void);
    try {
      cleanup = definition.mount(element);
    } catch (cause) {
      setFailure(cause instanceof Error ? cause.message : String(cause));
      return;
    }

    return () => {
      if (typeof cleanup === "function") {
        try {
          cleanup();
        } catch {
          // A failing cleanup must not stop the next panel from unmounting.
        }
      }
      element.innerHTML = "";
    };
  }, [definition]);

  if (failure) {
    return (
      <div className="error" style={{ fontSize: 12.5 }}>
        Panel “{definition.title}” failed to render: {failure}
      </div>
    );
  }
  return <div className="plugin-panel-body" ref={host} />;
}

export function PluginsView() {
  const runtimes = usePluginRuntime();
  const panels = pluginPanels();
  const failed = runtimes.filter((runtime) => runtime.status === "failed");

  return (
    <div className="stack">
      <div className="card panel">
        <h3 className="panel-title">Plugins</h3>
        {runtimes.length === 0 ? (
          <p className="muted" style={{ fontSize: 12.5, margin: 0 }}>
            No plugins installed. Drop a folder with <span className="mono">plugin.json</span> and{" "}
            <span className="mono">main.js</span> into the plugins directory, then enable it in
            Settings → Plugins.
          </p>
        ) : (
          <ul className="plugin-list">
            {runtimes.map((runtime) => (
              <li key={runtime.id} className="plugin-list-row">
                <span
                  className={
                    runtime.status === "running"
                      ? "badge badge-accent"
                      : runtime.status === "failed"
                        ? "badge badge-warn"
                        : "badge"
                  }
                >
                  {runtime.status}
                </span>
                <span className="plugin-list-name">{runtime.name}</span>
                <span className="muted mono" style={{ fontSize: 11.5 }}>
                  {runtime.id} · v{runtime.version}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      {failed.length > 0 ? (
        <div className="notice warn">
          {failed.map((runtime) => (
            <div key={runtime.id}>
              <strong>{runtime.name}</strong>: {runtime.error}
            </div>
          ))}
        </div>
      ) : null}

      {panels.length === 0 ? (
        runtimes.some((runtime) => runtime.status === "running") ? (
          <div className="card panel">
            <p className="muted" style={{ fontSize: 12.5, margin: 0 }}>
              Plugins are running but none registered a panel. They can still react to events —
              check Settings → Plugins for their hooks.
            </p>
          </div>
        ) : null
      ) : (
        panels.map(({ pluginId, pluginName, panel }) => (
          <div className="card panel" key={`${pluginId}:${panel.id}`}>
            <div className="row-between" style={{ marginBottom: 8 }}>
              <h3 className="panel-title" style={{ margin: 0 }}>
                {panel.title}
              </h3>
              <span className="muted" style={{ fontSize: 11.5 }}>
                <Icon name="package" size={12} /> {pluginName}
              </span>
            </div>
            <PanelSlot definition={panel} />
          </div>
        ))
      )}

      {runtimes.some((runtime) => runtime.notices.length > 0) ? (
        <div className="card panel">
          <h3 className="panel-title">Plugin output</h3>
          {runtimes
            .filter((runtime) => runtime.notices.length > 0)
            .map((runtime) => (
              <div key={runtime.id} style={{ marginBottom: 8 }}>
                <div className="muted" style={{ fontSize: 11.5, marginBottom: 4 }}>
                  {runtime.name}
                </div>
                <ul className="plugin-notices">
                  {runtime.notices.slice(0, 5).map((notice, index) => (
                    <li key={`${runtime.id}-${index}`}>{notice}</li>
                  ))}
                </ul>
              </div>
            ))}
        </div>
      ) : null}
    </div>
  );
}
