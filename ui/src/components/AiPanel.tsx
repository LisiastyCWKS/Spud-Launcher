import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import type { AiProvider, AiStatus, LauncherSettings } from "../lib/types";
import { Icon } from "./Icon";

const PROVIDERS: Array<{ id: AiProvider; label: string; hint: string }> = [
  {
    id: "openai",
    label: "OpenAI-compatible",
    hint: "OpenAI, Ollama, LM Studio, OpenRouter, Groq — anything that speaks /chat/completions.",
  },
  {
    id: "anthropic",
    label: "Anthropic",
    hint: "Claude through the /v1/messages endpoint.",
  },
];

/**
 * AI crash explanations.
 *
 * Deliberately two separate things: the fields here are ordinary settings that
 * are saved with the rest of the form, while the key is written straight to the
 * OS keyring and never read back into the DOM. The panel only ever shows
 * whether a key exists.
 */
export function AiPanel({
  settings,
  patch,
  onError,
}: {
  settings: LauncherSettings;
  patch: (next: Partial<LauncherSettings>) => void;
  onError: (message: string | null) => void;
}) {
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState<"save" | "clear" | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.ai.status());
    } catch (cause) {
      onError(String(cause));
    }
  }, [onError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const saveKey = async () => {
    if (!key.trim()) return;
    setBusy("save");
    onError(null);
    setNote(null);
    try {
      setStatus(await api.ai.setKey(key));
      // The key is gone from this point on: the input is cleared so it does not
      // linger in the DOM or in a component snapshot.
      setKey("");
      setNote("Key stored in the OS keyring.");
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const clearKey = async () => {
    setBusy("clear");
    onError(null);
    setNote(null);
    try {
      setStatus(await api.ai.clearKey());
      setNote("Key removed from the OS keyring.");
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const provider = PROVIDERS.find((entry) => entry.id === settings.ai_provider) ?? PROVIDERS[0];

  return (
    <div className="card settings-card">
      <label className="checkbox">
        <input
          type="checkbox"
          checked={settings.ai_enabled}
          onChange={(event) => patch({ ai_enabled: event.target.checked })}
        />
        Explain crashes with an AI provider
      </label>
      <span className="field-hint">
        Off by default. The offline rules always run; this only adds a written explanation. Nothing
        is sent without a separate confirmation for every single request.
      </span>

      <div className="field" style={{ marginTop: 14 }}>
        <label htmlFor="set-ai-provider">Provider</label>
        <select
          id="set-ai-provider"
          className="input"
          value={settings.ai_provider}
          onChange={(event) => patch({ ai_provider: event.target.value as AiProvider })}
        >
          {PROVIDERS.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.label}
            </option>
          ))}
        </select>
        <span className="field-hint">{provider.hint}</span>
      </div>

      <div className="field">
        <label htmlFor="set-ai-endpoint">Endpoint</label>
        <input
          id="set-ai-endpoint"
          className="input mono"
          value={settings.ai_endpoint}
          placeholder="https://api.openai.com/v1"
          onChange={(event) => patch({ ai_endpoint: event.target.value })}
        />
        <span className="field-hint">
          Base URL only. A local Ollama is <span className="mono">http://localhost:11434/v1</span>.
        </span>
      </div>

      <div className="row" style={{ gap: 12, alignItems: "flex-start" }}>
        <div className="field">
          <label htmlFor="set-ai-model">Model</label>
          <input
            id="set-ai-model"
            className="input mono"
            value={settings.ai_model}
            placeholder="gpt-4o-mini"
            onChange={(event) => patch({ ai_model: event.target.value })}
          />
        </div>
        <div className="field" style={{ maxWidth: 180 }}>
          <label htmlFor="set-ai-log">Prompt cap (KB)</label>
          <input
            id="set-ai-log"
            className="input mono"
            type="number"
            min={4}
            max={512}
            value={settings.ai_max_log_kb}
            onChange={(event) =>
              patch({ ai_max_log_kb: Number(event.target.value) || 64 })
            }
          />
        </div>
      </div>

      <div className="field" style={{ marginBottom: 0 }}>
        <label htmlFor="set-ai-key">API key</label>
        <div className="row" style={{ gap: 8 }}>
          <input
            id="set-ai-key"
            className="input mono"
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={key}
            placeholder={status?.has_key ? "•••••••• (stored)" : "paste the provider key"}
            onChange={(event) => setKey(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void saveKey();
            }}
          />
          <button
            type="button"
            className="btn"
            disabled={busy !== null || key.trim().length === 0}
            onClick={() => void saveKey()}
          >
            {busy === "save" ? <span className="spinner" /> : <Icon name="check" size={15} />}
            Save
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={busy !== null || !status?.has_key}
            onClick={() => void clearKey()}
          >
            {busy === "clear" ? <span className="spinner" /> : <Icon name="trash" size={15} />}
            Clear
          </button>
        </div>
        <span className="field-hint">
          {status === null
            ? "Checking the OS keyring…"
            : !status.keyring
              ? "No OS credential store is available here, so the key cannot be saved. AI explanations stay off."
              : status.has_key
                ? "Stored in the OS keyring — never in settings.json, never in a log."
                : "No key stored yet. Enter one above to enable the explainer."}
          {note ? ` ${note}` : ""}
        </span>
      </div>

      <div className="section-sub" style={{ marginTop: 12, marginBottom: 0 }}>
        Before sending, the report is stripped of your home path, access tokens, session ids,
        player UUIDs and e-mail addresses. You see the redacted text and confirm it in the crash
        dialog.
      </div>
    </div>
  );
}
