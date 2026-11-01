import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { Icon } from "./Icon";
import { BrandMark } from "./BrandMark";
import type { AccountKind, AccountsView, Theme } from "../lib/types";
import { classNames } from "../lib/format";

export type View =
  | "instances"
  | "discover"
  | "import"
  | "friends"
  | "diagnostics"
  | "accounts"
  | "settings";

type NavIcon =
  | "grid"
  | "compass"
  | "user"
  | "settings"
  | "package"
  | "warning"
  | "copy";

const LIBRARY: Array<{ id: View; label: string; icon: NavIcon }> = [
  { id: "instances", label: "Instances", icon: "grid" },
  { id: "discover", label: "Discover", icon: "compass" },
  { id: "import", label: "Import", icon: "package" },
  { id: "friends", label: "Friends", icon: "copy" },
  { id: "diagnostics", label: "Diagnostics", icon: "warning" },
];

const SYSTEM: Array<{ id: View; label: string; icon: NavIcon }> = [
  { id: "accounts", label: "Accounts", icon: "user" },
  { id: "settings", label: "Settings", icon: "settings" },
];

export function Sidebar({
  view,
  onNavigate,
  accountName,
  accountKind,
  accounts,
  onAccounts,
  theme,
  onCycleTheme,
}: {
  view: View;
  onNavigate: (view: View) => void;
  accountName: string | null;
  accountKind: AccountKind | null;
  accounts: AccountsView | null;
  onAccounts: (view: AccountsView) => void;
  theme: Theme;
  onCycleTheme: () => void;
}) {
  const themeIcon = theme === "dark" ? "moon" : theme === "light" ? "sun" : "monitor";
  const themeLabel =
    theme === "dark" ? "Dark theme" : theme === "light" ? "Light theme" : "System theme";

  const [switcher, setSwitcher] = useState(false);
  const [switching, setSwitching] = useState<string | null>(null);
  const chip = useRef<HTMLDivElement | null>(null);

  // A popover that survives a click outside is a popover that never leaves.
  useEffect(() => {
    if (!switcher) return;
    const onDown = (event: MouseEvent) => {
      if (chip.current && !chip.current.contains(event.target as Node)) setSwitcher(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setSwitcher(false);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [switcher]);

  const switchTo = async (id: string) => {
    setSwitching(id);
    try {
      onAccounts(await api.accounts.setActive(id));
      setSwitcher(false);
    } catch {
      // A failed switch leaves the previous account active; the panel keeps
      // showing the real state, so there is nothing to roll back here.
    } finally {
      setSwitching(null);
    }
  };

  const list = accounts?.accounts ?? [];

  return (
    <aside className="sidebar">
      <div className="brand">
        <BrandMark />
        <div>
          <div className="brand-name">spud</div>
          <div className="brand-sub">Minecraft launcher</div>
        </div>
      </div>

      <nav className="nav" aria-label="Main">
        <div className="nav-label">Library</div>
        {LIBRARY.map((item) => (
          <button
            key={item.id}
            type="button"
            className={classNames("nav-item", view === item.id && "active")}
            onClick={() => onNavigate(item.id)}
            title={item.label}
          >
            <Icon name={item.icon} />
            <span>{item.label}</span>
          </button>
        ))}

        <div className="nav-label" style={{ marginTop: 14 }}>
          You
        </div>
        {SYSTEM.map((item) => (
          <button
            key={item.id}
            type="button"
            className={classNames("nav-item", view === item.id && "active")}
            onClick={() => onNavigate(item.id)}
            title={item.label}
          >
            <Icon name={item.icon} />
            <span>{item.label}</span>
          </button>
        ))}
      </nav>

      <div className="sidebar-spacer" />

      <div className="sidebar-foot">
        <button type="button" className="nav-item" onClick={onCycleTheme} title={themeLabel}>
          <Icon name={themeIcon} />
          <span>{themeLabel}</span>
        </button>

        <div className="account-switch" ref={chip}>
          {switcher ? (
            <div className="account-menu" role="menu">
              <div className="account-menu-label">Switch account</div>
              {list.length === 0 ? (
                <div className="account-menu-empty">No accounts yet.</div>
              ) : (
                list.map((account) => (
                  <button
                    key={account.id}
                    type="button"
                    role="menuitem"
                    className={classNames("account-menu-item", account.active && "active")}
                    disabled={switching !== null}
                    onClick={() => void switchTo(account.id)}
                  >
                    <div className="avatar small">{account.username.slice(0, 1).toUpperCase()}</div>
                    <div className="account-menu-text">
                      <div className="account-menu-name">{account.username}</div>
                      <div className="account-menu-sub">
                        {account.kind === "offline" ? "Offline" : "Microsoft"}
                      </div>
                    </div>
                    {switching === account.id ? (
                      <span className="spinner" />
                    ) : account.active ? (
                      <Icon name="check" size={14} />
                    ) : null}
                  </button>
                ))
              )}
              <div className="account-menu-sep" />
              <button
                type="button"
                role="menuitem"
                className="account-menu-item"
                onClick={() => {
                  setSwitcher(false);
                  onNavigate("accounts");
                }}
              >
                <Icon name="user" size={15} />
                <div className="account-menu-text">
                  <div className="account-menu-name">Manage accounts</div>
                  <div className="account-menu-sub">Add Microsoft or offline</div>
                </div>
              </button>
            </div>
          ) : null}

          <button
            type="button"
            className="account-chip"
            onClick={() => setSwitcher((value) => !value)}
            title={accountName ? "Switch account" : "Sign in"}
          >
            <div className="avatar">
              {accountName ? accountName.slice(0, 1).toUpperCase() : "?"}
            </div>
            <div className="account-chip-text">
              <div className="account-chip-name">{accountName ?? "Not signed in"}</div>
              <div className="account-chip-sub">
                {accountName
                  ? accountKind === "offline"
                    ? "Offline account"
                    : "Microsoft account"
                  : "Add an account"}
              </div>
            </div>
            {list.length > 1 ? <span className="badge">{list.length}</span> : null}
          </button>
        </div>
      </div>
    </aside>
  );
}
