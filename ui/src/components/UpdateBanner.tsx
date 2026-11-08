import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { t } from "../lib/i18n";
import type { UpdateInfo } from "../lib/types";
import { Icon } from "./Icon";

/**
 * A thin banner shown once per session when a newer release is available.
 * It checks GitHub Releases on mount and disappears after the user dismisses it
 * or opens the download link.
 */
export function UpdateBanner() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    // A failed check is silently ignored — no network = no banner, not an error.
    api.update
      .check()
      .then((result) => {
        if (result.available) setInfo(result);
      })
      .catch(() => undefined);
  }, []);

  if (!info || dismissed) return null;

  return (
    <div className="update-banner" role="status" aria-live="polite">
      <span className="update-dot" aria-hidden="true">
        <Icon name="download" size={13} />
      </span>
      <span className="update-copy">
        <strong>{t("update.available")}</strong>{" "}
        {t("update.description", { version: info.latest })}
      </span>
      <a
        className="btn btn-sm update-action"
        href={info.url}
        target="_blank"
        rel="noreferrer"
        onClick={() => setDismissed(true)}
      >
        {t("update.download")}
        <Icon name="external" size={13} />
      </a>
      <button
        type="button"
        className="icon-btn update-dismiss"
        aria-label={t("update.dismiss")}
        title={t("update.dismiss")}
        onClick={() => setDismissed(true)}
      >
        <Icon name="close" size={14} />
      </button>
    </div>
  );
}
