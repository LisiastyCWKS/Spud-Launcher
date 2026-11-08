import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, FormEvent } from "react";
import { api } from "../lib/api";
import { confirmDialog } from "../lib/confirm";
import { t } from "../lib/i18n";
import { copyText } from "../lib/format";
import type { AccountsView, AuthStatus, DeviceCode } from "../lib/types";
import { Icon } from "./Icon";
import { Modal } from "./Modal";

export function AccountsPanel({
  accounts,
  authStatus,
  onAccounts,
}: {
  accounts: AccountsView | null;
  authStatus: AuthStatus | null;
  onAccounts: (view: AccountsView) => void;
}) {
  const [device, setDevice] = useState<DeviceCode | null>(null);
  const [waiting, setWaiting] = useState(false);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [remaining, setRemaining] = useState(0);
  const [nickname, setNickname] = useState("");
  const [savingOffline, setSavingOffline] = useState(false);
  const attempt = useRef(0);

  const refresh = useCallback(async () => {
    try {
      onAccounts(await api.accounts.list());
    } catch (cause) {
      setError(String(cause));
    }
  }, [onAccounts]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!device) return;
    setRemaining(device.expires_in);
    const timer = window.setInterval(() => {
      setRemaining((value) => (value > 0 ? value - 1 : 0));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [device]);

  const begin = async () => {
    const generation = ++attempt.current;
    setError(null);
    setCopied(false);
    setWaiting(true);
    try {
      const code = await api.accounts.beginLogin();
      if (generation !== attempt.current) return;
      setDevice(code);
      try {
        await api.accounts.finishLogin(code);
        if (generation !== attempt.current) return;
        setDevice(null);
        await refresh();
      } catch (cause) {
        if (generation !== attempt.current) return;
        setError(String(cause));
      } finally {
        if (generation === attempt.current) setWaiting(false);
      }
    } catch (cause) {
      if (generation !== attempt.current) return;
      setError(String(cause));
      setWaiting(false);
    }
  };

  const cancelLogin = () => {
    attempt.current += 1;
    setWaiting(false);
    setDevice(null);
    setError(null);
  };

  const addOffline = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = nickname.trim();
    if (!name) return;
    setSavingOffline(true);
    setError(null);
    try {
      onAccounts(await api.accounts.addOffline(name));
      setNickname("");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSavingOffline(false);
    }
  };

  const setActive = async (id: string) => {
    setPending(id);
    setError(null);
    try {
      onAccounts(await api.accounts.setActive(id));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPending(null);
    }
  };

  const remove = async (id: string) => {
    const confirmed = await confirmDialog({
      title: t("confirm.remove_account_title"),
      description: t("confirm.remove_account_body"),
      confirmLabel: t("common.delete"),
      cancelLabel: t("common.cancel"),
      tone: "danger",
    });
    if (!confirmed) return;
    setPending(id);
    setError(null);
    try {
      onAccounts(await api.accounts.remove(id));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPending(null);
    }
  };

  return (
    <div className="stack" style={{ maxWidth: 780 }}>
      <div className="row-between">
        <div>
          <h3 className="section-title">Accounts</h3>
          <div className="section-sub">
            Microsoft tokens live in the OS keyring — never in a config file. Offline accounts store
            only a nickname. Multiple accounts can be stored; one is active at a time.
          </div>
        </div>
        <button type="button" className="btn btn-primary" onClick={begin} disabled={waiting}>
          {waiting ? <span className="spinner" /> : <Icon name="plus" size={15} />}
          Add Microsoft account
        </button>
      </div>

      {authStatus && !authStatus.configured ? (
        <div className="notice warn">
          Microsoft sign-in is not configured yet — add an Azure application (client) ID in
          Settings → Microsoft sign-in, or set <code>SPUD_MS_CLIENT_ID</code>. Offline nicknames work
          without it.
        </div>
      ) : null}

      {error && !device ? <div className="error">{error}</div> : null}

      {accounts && accounts.accounts.length > 0 ? (
        <div className="card">
          {accounts.accounts.map((account, index) => (
            <div
              key={account.id}
              className="account-row"
              style={{ "--i": index } as CSSProperties}
            >
              <div className="account-avatar">{account.username.slice(0, 1).toUpperCase()}</div>
              <div style={{ minWidth: 0, flex: 1 }}>
                <div className="account-name">{account.username}</div>
                <div className="muted mono" style={{ fontSize: 11.5 }}>
                  {account.uuid}
                </div>
              </div>
              <span
                className={account.kind === "offline" ? "badge" : "badge badge-accent"}
                title={
                  account.kind === "offline"
                    ? "Offline nickname — no Microsoft session, works without a network"
                    : "Microsoft account — skins, Realms and online servers work"
                }
              >
                {account.kind === "offline" ? "Offline" : "Microsoft"}
              </span>
              {account.active ? (
                <span className="badge badge-accent">
                  <Icon name="check" size={12} /> Active
                </span>
              ) : (
                <button
                  type="button"
                  className="btn btn-sm"
                  onClick={() => setActive(account.id)}
                  disabled={pending !== null}
                >
                  {pending === account.id ? <span className="spinner" /> : null}
                  Use
                </button>
              )}
              <button
                type="button"
                className="btn btn-sm btn-danger"
                onClick={() => remove(account.id)}
                disabled={pending !== null}
              >
                <Icon name="trash" size={14} />
              </button>
            </div>
          ))}
        </div>
      ) : (
        <div className="empty">
          <h3>No account yet</h3>
          <p style={{ margin: "0 auto", maxWidth: 440 }}>
            Sign in with Microsoft to launch the game. The device-code flow opens no local server —
            you enter a short code on microsoft.com instead.
          </p>
        </div>
      )}

      <form className="card stack" onSubmit={addOffline} style={{ padding: 16 }}>
        <div>
          <h3 className="section-title" style={{ marginBottom: 4 }}>
            Offline account
          </h3>
          <div className="section-sub">
            Pick a nickname and play without signing in. The UUID is derived the same way the vanilla
            server derives offline players, so single-player worlds keep the same identity on every
            machine. Online servers will reject the session.
          </div>
        </div>
        <div className="row" style={{ gap: 10, alignItems: "flex-end" }}>
          <div className="field" style={{ flex: 1 }}>
            <label htmlFor="offline-nick">Nickname</label>
            <input
              id="offline-nick"
              className="input"
              value={nickname}
              onChange={(event) => setNickname(event.target.value)}
              placeholder="3–16 characters, letters, digits, underscore"
              maxLength={16}
              autoComplete="off"
              spellCheck={false}
            />
          </div>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={savingOffline || nickname.trim().length < 3}
          >
            {savingOffline ? <span className="spinner" /> : <Icon name="user" size={15} />}
            Add offline
          </button>
        </div>
      </form>

      <div className="notice">
        Offline accounts carry no Microsoft session: skins fall back to the default, Realms and
        online servers reject the login, and no token is ever written to disk. Everything else —
        installing, mods, single-player — works normally.
      </div>

      {device ? (
        <Modal
          title="Sign in with Microsoft"
          subtitle={`Code expires in ${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, "0")}`}
          onClose={cancelLogin}
          width={460}
        >
          <div className="stack">
            <p className="soft" style={{ margin: 0, fontSize: 13 }}>
              Open{" "}
              <a href={device.verification_uri} target="_blank" rel="noreferrer">
                {device.verification_uri}
              </a>{" "}
              and enter this code:
            </p>

            <button
              type="button"
              className="code-box"
              onClick={async () => {
                await copyText(device.user_code);
                setCopied(true);
              }}
              title="Copy the code"
            >
              {device.user_code}
            </button>

            <div className="row" style={{ justifyContent: "center" }}>
              <button
                type="button"
                className="btn btn-sm"
                onClick={async () => {
                  await copyText(device.user_code);
                  setCopied(true);
                }}
              >
                <Icon name={copied ? "check" : "copy"} size={14} />
                {copied ? "Copied" : "Copy code"}
              </button>
            </div>

            <div className="notice">
              {waiting ? (
                <span className="row" style={{ gap: 8 }}>
                  <span className="spinner" /> Waiting for approval…
                </span>
              ) : error ? (
                <span className="error">{error}</span>
              ) : (
                "Approve the sign-in in your browser to finish."
              )}
            </div>

            <div className="modal-actions">
              <button type="button" className="btn" onClick={cancelLogin}>
                Cancel
              </button>
            </div>
          </div>
        </Modal>
      ) : null}
    </div>
  );
}
