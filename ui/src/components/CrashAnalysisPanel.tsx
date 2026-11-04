import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { bytes } from "../lib/format";
import type { CrashAnalysis, LogFileInfo, Severity } from "../lib/types";
import { Icon } from "./Icon";
import { Modal } from "./Modal";

const SEVERITY_STYLE: Record<Severity, { label: string; color: string }> = {
  critical: { label: "critical", color: "var(--danger)" },
  warning: { label: "warning", color: "var(--warning)" },
  info: { label: "info", color: "var(--muted)" },
};

/**
 * Crash analysis dialog.
 *
 * Two calls, in this order, never in parallel: the first one is local only and
 * also fetches the redacted preview, the second one is the one that actually
 * talks to the provider and only runs after the user presses the confirm
 * button. Closing the dialog cancels the consent — it is never remembered.
 */
export function CrashAnalysisPanel({
  instanceId,
  file,
  onClose,
  onError,
}: {
  instanceId: string;
  file: LogFileInfo;
  onClose: () => void;
  onError: (message: string) => void;
}) {
  const [analysis, setAnalysis] = useState<CrashAnalysis | null>(null);
  const [busy, setBusy] = useState<"local" | "ai" | null>("local");

  const run = async (consent: boolean) => {
    setBusy(consent ? "ai" : "local");
    try {
      setAnalysis(await api.ai.analyze(instanceId, file.name, consent));
    } catch (cause) {
      onError(String(cause));
      if (!consent) onClose();
    } finally {
      setBusy(null);
    }
  };

  useEffect(() => {
    void run(false);
    // The dialog is keyed by the file in the parent, so this runs once per file.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const local = analysis?.local ?? null;

  return (
    <Modal
      title={`Analysis — ${file.name.replace(/^(crash-reports|logs)\//, "")}`}
      subtitle={file.path}
      width={820}
      onClose={onClose}
    >
      {busy === "local" || !local ? (
        <div className="row" style={{ gap: 8, padding: "18px 0" }}>
          <span className="spinner" />
          <span className="muted">Reading the report and running the offline rules…</span>
        </div>
      ) : (
        <div className="stack" style={{ gap: 14 }}>
          <div className="panel" style={{ margin: 0 }}>
            <div className="panel-title">What happened</div>
            <div style={{ fontSize: 13, lineHeight: 1.55 }}>{local.summary}</div>
            {local.exception ? (
              <div className="muted mono" style={{ fontSize: 11.5, marginTop: 6, wordBreak: "break-word" }}>
                {local.exception}
              </div>
            ) : null}
            <div className="muted" style={{ fontSize: 11.5, marginTop: 8 }}>
              {bytes(local.bytes)} examined
              {local.truncated ? " (only the tail — the start was skipped)" : ""} ·{" "}
              {local.diagnoses.length} rule{local.diagnoses.length === 1 ? "" : "s"} matched
            </div>
          </div>

          {local.diagnoses.length === 0 ? (
            <div className="empty small">
              No known pattern matched. The log may be from a successful run, or the failure is
              something the rules do not recognise yet.
            </div>
          ) : (
            local.diagnoses.map((diagnosis) => {
              const style = SEVERITY_STYLE[diagnosis.severity];
              return (
                <div key={diagnosis.id} className="panel" style={{ margin: 0 }}>
                  <div className="row-between" style={{ alignItems: "flex-start", gap: 10 }}>
                    <div style={{ fontSize: 13.5, fontWeight: 600 }}>{diagnosis.title}</div>
                    <span
                      className="chip"
                      style={{ color: style.color, borderColor: style.color, flexShrink: 0 }}
                    >
                      {style.label}
                    </span>
                  </div>
                  <pre
                    className="code-box mono"
                    style={{ marginTop: 8, marginBottom: 8, maxHeight: 120, overflow: "auto" }}
                  >
                    {diagnosis.evidence}
                  </pre>
                  <ol style={{ margin: 0, paddingLeft: 18, fontSize: 12.5, lineHeight: 1.6 }}>
                    {diagnosis.fix.map((step, index) => (
                      <li key={index}>{step}</li>
                    ))}
                  </ol>
                </div>
              );
            })
          )}

          {local.mods.length > 0 ? (
            <div>
              <div className="section-sub" style={{ marginBottom: 6 }}>
                Mods mentioned in the log
              </div>
              <div className="chips">
                {local.mods.map((name) => (
                  <span key={name} className="chip mono" style={{ cursor: "default" }}>
                    {name}
                  </span>
                ))}
              </div>
            </div>
          ) : null}

          {analysis?.ai ? (
            <div className="panel" style={{ margin: 0 }}>
              <div className="panel-title">AI explanation</div>
              <div style={{ fontSize: 13, lineHeight: 1.65, whiteSpace: "pre-wrap" }}>
                {analysis.ai}
              </div>
            </div>
          ) : null}

          {!analysis?.ai && analysis?.preview !== null && analysis?.preview !== undefined ? (
            <div className="panel" style={{ margin: 0 }}>
              <div className="panel-title">Send this to the provider?</div>
              <div className="section-sub">
                {analysis.redaction
                  ? `Redacted before sending: ${analysis.redaction.tokens} token(s), ${analysis.redaction.home_paths} home path(s), ${analysis.redaction.uuids} UUID(s), ${analysis.redaction.emails} e-mail(s).`
                  : "The report is redacted before sending."}{" "}
                The provider sees the text below and the offline diagnoses above — nothing else.
                This confirmation is per request.
              </div>
              <pre
                className="code-box mono"
                style={{ marginTop: 8, maxHeight: 220, overflow: "auto", whiteSpace: "pre-wrap" }}
              >
                {analysis.preview}
              </pre>
              <div className="modal-actions" style={{ marginTop: 10 }}>
                <button type="button" className="btn btn-ghost" onClick={onClose}>
                  Keep it local
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={busy !== null}
                  onClick={() => void run(true)}
                >
                  {busy === "ai" ? <span className="spinner" /> : <Icon name="warning" size={15} />}
                  Send redacted log and explain
                </button>
              </div>
            </div>
          ) : null}

          {!analysis?.ai && analysis?.ai_error ? (
            <div className="section-sub" style={{ margin: 0 }}>
              {analysis.ai_error}
            </div>
          ) : null}
        </div>
      )}
    </Modal>
  );
}
