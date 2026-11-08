/**
 * Minimal i18n system.
 *
 * Usage:
 *   import { t } from "../lib/i18n";
 *   <h1>{t("instances.title")}</h1>
 *
 * The locale is driven by LauncherSettings.locale; "" falls back to "en-US".
 * Call `setLocale(locale)` once on app boot (and on settings save).
 */

// ---------------------------------------------------------------------------
// Translation tables
// ---------------------------------------------------------------------------

type Dict = Record<string, string>;

const EN: Dict = {
  // Sidebar
  "nav.instances": "Instances",
  "nav.discover": "Discover",
  "nav.import": "Import",
  "nav.friends": "Friends",
  "nav.diagnostics": "Diagnostics",
  "nav.plugins": "Plugins",
  "nav.accounts": "Accounts",
  "nav.settings": "Settings",
  "nav.library": "Library",
  "nav.you": "You",
  // Topbar
  "instances.title": "Instances",
  "instances.subtitle_one": "1 instance · shared libraries and assets",
  "instances.subtitle_many": "{n} instances · shared libraries and assets",
  "instances.new": "New instance",
  "discover.title": "Discover",
  "discover.subtitle": "Mods, modpacks, resource packs and shaders from Modrinth",
  "import.title": "Import",
  "import.subtitle": "Other launchers on this PC, or a .zip / .mrpack from a friend",
  "friends.title": "Friends",
  "friends.subtitle": "Who is online, which server they are on, and one-click instance copies",
  "diagnostics.title": "Diagnostics",
  "diagnostics.subtitle": "Live console and the crash logs left by the last runs",
  "plugins.title": "Plugins",
  "plugins.subtitle": "Extensions you installed and what they added to the launcher",
  "accounts.title": "Accounts",
  "accounts.subtitle": "Microsoft sign-in or a local offline nickname",
  "settings.title": "Settings",
  "settings.subtitle": "Downloads, Java runtimes, endpoints and data",
  "instances.ready": "ready",
  "instances.not_installed": "not installed",
  "instances.empty_title": "No instances yet",
  "instances.empty_body":
    "Every instance gets its own mods, resource packs and settings. Create one and pick a mod loader — Fabric, Quilt, Forge, NeoForge or plain vanilla.",
  "instances.empty_action": "Create instance",
  // Shell chrome
  "shell.fullscreen": "Fullscreen (F11)",
  "shell.leave_fullscreen": "Leave fullscreen (Esc)",
  "shell.brand_sub": "Minecraft launcher",
  "shell.theme_dark": "Dark theme",
  "shell.theme_light": "Light theme",
  "shell.theme_system": "System theme",
  "shell.switch_account": "Switch account",
  "shell.no_accounts": "No accounts yet.",
  "shell.manage_accounts": "Manage accounts",
  "shell.manage_accounts_sub": "Add Microsoft or offline",
  "shell.offline": "Offline",
  "shell.microsoft": "Microsoft",
  "shell.sign_in": "Sign in",
  "shell.offline_account": "Offline account",
  "shell.microsoft_account": "Microsoft account",
  // Confirmations
  "common.delete": "Delete",
  "confirm.delete_title": 'Delete "{name}"?',
  "confirm.delete_body":
    "Every file inside its .minecraft folder is removed with it. This cannot be undone.",
  "confirm.delete_running": "The running game is stopped first.",
  "confirm.remove_account_title": "Remove this account?",
  "confirm.remove_account_body": "Its stored tokens are deleted from this machine.",
  "confirm.delete_snapshot_title": "Delete this snapshot?",
  "confirm.delete_snapshot_body": "The copy on the remote is removed. This cannot be undone.",
  // Toasts
  "toast.download_done": "Download complete",
  "toast.download_done_text": "{n} files verified and in place.",
  "toast.files": "{n} files",
  "toast.settings_saved": "Settings saved",
  "toast.settings_saved_text": "Every change is live — no restart needed.",
  "toast.instance_created": "Instance created",
  "toast.instance_deleted": "Instance deleted",
  "toast.installed": "Files installed",
  "toast.launched": "Minecraft is starting",
  "toast.stopped": "Game stopped",
  "toast.folder_copied": "Folder path copied",
  "toast.exported": "Instance exported",
  "toast.refreshed": "Refreshed",
  // Update banner
  "update.available": "Update available",
  "update.description": "Version {version} is out.",
  "update.download": "Download",
  "update.dismiss": "Dismiss",
  // CurseForge panel
  "cf.no_key": "No CurseForge API key — add it in Settings → Endpoints.",
  "cf.open_page": "Open on CurseForge",
  "cf.downloads": "{n} downloads",
  "cf.by": "by {author}",
  // Common
  "common.refresh": "Refresh",
  "common.save": "Save settings",
  "common.saved": "Saved",
  "common.loading": "Loading…",
  "common.error": "Error",
  "common.dismiss": "Dismiss",
  "common.back": "Back",
  "common.cancel": "Cancel",
  "common.confirm": "Confirm",
  "common.search": "Search",
  "common.not_signed_in": "Not signed in",
  "common.add_account": "Add an account",
};

const PL: Dict = {
  // Sidebar
  "nav.instances": "Instancje",
  "nav.discover": "Odkryj",
  "nav.import": "Importuj",
  "nav.friends": "Znajomi",
  "nav.diagnostics": "Diagnostyka",
  "nav.plugins": "Wtyczki",
  "nav.accounts": "Konta",
  "nav.settings": "Ustawienia",
  "nav.library": "Biblioteka",
  "nav.you": "Ty",
  // Topbar
  "instances.title": "Instancje",
  "instances.subtitle_one": "1 instancja · wspólne biblioteki i assety",
  "instances.subtitle_many": "{n} instancji · wspólne biblioteki i assety",
  "instances.new": "Nowa instancja",
  "discover.title": "Odkryj",
  "discover.subtitle": "Mody, modpacki, resource packi i shadery z Modrintha",
  "import.title": "Importuj",
  "import.subtitle": "Inne launchery na tym PC albo .zip / .mrpack od znajomego",
  "friends.title": "Znajomi",
  "friends.subtitle": "Kto jest online, na jakim serwerze gra i kopiowanie instancji jednym kliknięciem",
  "diagnostics.title": "Diagnostyka",
  "diagnostics.subtitle": "Konsola na żywo i logi crashy z ostatnich uruchomień",
  "plugins.title": "Wtyczki",
  "plugins.subtitle": "Rozszerzenia, które zainstalowałeś i co dodały do launchera",
  "accounts.title": "Konta",
  "accounts.subtitle": "Logowanie Microsoft lub lokalny nick offline",
  "settings.title": "Ustawienia",
  "settings.subtitle": "Pobieranie, środowiska Java, adresy endpointów i dane",
  "instances.ready": "gotowa",
  "instances.not_installed": "niezainstalowana",
  "instances.empty_title": "Nie ma jeszcze instancji",
  "instances.empty_body":
    "Każda instancja ma własne mody, resource packi i ustawienia. Utwórz jedną i wybierz loader — Fabric, Quilt, Forge, NeoForge albo czysty vanilla.",
  "instances.empty_action": "Utwórz instancję",
  "shell.fullscreen": "Pełny ekran (F11)",
  "shell.leave_fullscreen": "Wyjdź z pełnego ekranu (Esc)",
  "shell.brand_sub": "Launcher Minecraft",
  "shell.theme_dark": "Motyw ciemny",
  "shell.theme_light": "Motyw jasny",
  "shell.theme_system": "Motyw systemowy",
  "shell.switch_account": "Przełącz konto",
  "shell.no_accounts": "Brak kont.",
  "shell.manage_accounts": "Zarządzaj kontami",
  "shell.manage_accounts_sub": "Dodaj Microsoft albo offline",
  "shell.offline": "Offline",
  "shell.microsoft": "Microsoft",
  "shell.sign_in": "Zaloguj się",
  "shell.offline_account": "Konto offline",
  "shell.microsoft_account": "Konto Microsoft",
  "common.delete": "Usuń",
  "confirm.delete_title": 'Usunąć instancję „{name}”?',
  "confirm.delete_body":
    "Wszystkie pliki w jej folderze .minecraft znikną razem z nią. Tego nie da się cofnąć.",
  "confirm.delete_running": "Działająca gra zostanie najpierw zatrzymana.",
  "confirm.remove_account_title": "Usunąć to konto?",
  "confirm.remove_account_body": "Zapisane tokeny zostaną usunięte z tego komputera.",
  "confirm.delete_snapshot_title": "Usunąć ten snapshot?",
  "confirm.delete_snapshot_body": "Kopia na zdalnym dysku zostanie usunięta. Tego nie da się cofnąć.",
  "toast.download_done": "Pobieranie zakończone",
  "toast.download_done_text": "{n} plików sprawdzonych i na miejscu.",
  "toast.files": "{n} plików",
  "toast.settings_saved": "Ustawienia zapisane",
  "toast.settings_saved_text": "Wszystkie zmiany działają od razu — bez restartu.",
  "toast.instance_created": "Instancja utworzona",
  "toast.instance_deleted": "Instancja usunięta",
  "toast.installed": "Pliki zainstalowane",
  "toast.launched": "Minecraft się uruchamia",
  "toast.stopped": "Gra zatrzymana",
  "toast.folder_copied": "Ścieżka skopiowana",
  "toast.exported": "Instancja wyeksportowana",
  "toast.refreshed": "Odświeżono",
  // Update banner
  "update.available": "Dostępna aktualizacja",
  "update.description": "Wersja {version} jest dostępna.",
  "update.download": "Pobierz",
  "update.dismiss": "Zamknij",
  // CurseForge panel
  "cf.no_key": "Brak klucza API CurseForge — dodaj go w Ustawienia → Endpointy.",
  "cf.open_page": "Otwórz na CurseForge",
  "cf.downloads": "{n} pobrań",
  "cf.by": "autorstwa {author}",
  // Common
  "common.refresh": "Odśwież",
  "common.save": "Zapisz ustawienia",
  "common.saved": "Zapisano",
  "common.loading": "Ładowanie…",
  "common.error": "Błąd",
  "common.dismiss": "Zamknij",
  "common.back": "Wstecz",
  "common.cancel": "Anuluj",
  "common.confirm": "Potwierdź",
  "common.search": "Szukaj",
  "common.not_signed_in": "Nie zalogowano",
  "common.add_account": "Dodaj konto",
};

const DE: Dict = {
  "nav.instances": "Instanzen",
  "nav.discover": "Entdecken",
  "nav.import": "Importieren",
  "nav.friends": "Freunde",
  "nav.diagnostics": "Diagnose",
  "nav.plugins": "Plugins",
  "nav.accounts": "Konten",
  "nav.settings": "Einstellungen",
  "nav.library": "Bibliothek",
  "nav.you": "Du",
  "instances.title": "Instanzen",
  "instances.subtitle_one": "1 Instanz · gemeinsame Bibliotheken und Assets",
  "instances.subtitle_many": "{n} Instanzen · gemeinsame Bibliotheken und Assets",
  "instances.new": "Neue Instanz",
  "discover.title": "Entdecken",
  "discover.subtitle": "Mods, Modpacks, Ressourcenpakete und Shader von Modrinth",
  "import.title": "Importieren",
  "import.subtitle": "Andere Launcher auf diesem PC oder eine .zip / .mrpack von einem Freund",
  "friends.title": "Freunde",
  "friends.subtitle": "Wer online ist, auf welchem Server und Instanzen mit einem Klick kopieren",
  "diagnostics.title": "Diagnose",
  "diagnostics.subtitle": "Live-Konsole und Absturzprotokolle der letzten Läufe",
  "plugins.title": "Plugins",
  "plugins.subtitle": "Installierte Erweiterungen und was sie dem Launcher hinzugefügt haben",
  "accounts.title": "Konten",
  "accounts.subtitle": "Microsoft-Anmeldung oder ein lokaler Offline-Nickname",
  "settings.title": "Einstellungen",
  "settings.subtitle": "Downloads, Java-Laufzeiten, Endpunkte und Daten",
  "instances.ready": "bereit",
  "instances.not_installed": "nicht installiert",
  "instances.empty_title": "Noch keine Instanzen",
  "instances.empty_body":
    "Jede Instanz hat eigene Mods, Ressourcenpakete und Einstellungen. Erstelle eine und wähle einen Loader — Fabric, Quilt, Forge, NeoForge oder pures Vanilla.",
  "instances.empty_action": "Instanz erstellen",
  "shell.fullscreen": "Vollbild (F11)",
  "shell.leave_fullscreen": "Vollbild verlassen (Esc)",
  "shell.brand_sub": "Minecraft-Launcher",
  "shell.theme_dark": "Dunkles Design",
  "shell.theme_light": "Helles Design",
  "shell.theme_system": "Systemdesign",
  "shell.switch_account": "Konto wechseln",
  "shell.no_accounts": "Noch keine Konten.",
  "shell.manage_accounts": "Konten verwalten",
  "shell.manage_accounts_sub": "Microsoft oder offline hinzufügen",
  "shell.offline": "Offline",
  "shell.microsoft": "Microsoft",
  "shell.sign_in": "Anmelden",
  "shell.offline_account": "Offline-Konto",
  "shell.microsoft_account": "Microsoft-Konto",
  "common.delete": "Löschen",
  "confirm.delete_title": "„{name}“ löschen?",
  "confirm.delete_body":
    "Alle Dateien in ihrem .minecraft-Ordner werden mitgelöscht. Das lässt sich nicht rückgängig machen.",
  "confirm.delete_running": "Das laufende Spiel wird zuerst beendet.",
  "confirm.remove_account_title": "Dieses Konto entfernen?",
  "confirm.remove_account_body": "Die gespeicherten Tokens werden von diesem Rechner gelöscht.",
  "confirm.delete_snapshot_title": "Diesen Snapshot löschen?",
  "confirm.delete_snapshot_body":
    "Die Kopie auf dem Remote-Speicher wird entfernt. Das lässt sich nicht rückgängig machen.",
  "toast.download_done": "Download abgeschlossen",
  "toast.download_done_text": "{n} Dateien geprüft und vorhanden.",
  "toast.files": "{n} Dateien",
  "toast.settings_saved": "Einstellungen gespeichert",
  "toast.settings_saved_text": "Alle Änderungen sind sofort aktiv — kein Neustart nötig.",
  "toast.instance_created": "Instanz erstellt",
  "toast.instance_deleted": "Instanz gelöscht",
  "toast.installed": "Dateien installiert",
  "toast.launched": "Minecraft startet",
  "toast.stopped": "Spiel beendet",
  "toast.folder_copied": "Pfad kopiert",
  "toast.exported": "Instanz exportiert",
  "toast.refreshed": "Aktualisiert",
  "update.available": "Update verfügbar",
  "update.description": "Version {version} ist verfügbar.",
  "update.download": "Herunterladen",
  "update.dismiss": "Schließen",
  "cf.no_key": "Kein CurseForge API-Schlüssel — in Einstellungen → Endpunkte hinzufügen.",
  "cf.open_page": "Auf CurseForge öffnen",
  "cf.downloads": "{n} Downloads",
  "cf.by": "von {author}",
  "common.refresh": "Aktualisieren",
  "common.save": "Einstellungen speichern",
  "common.saved": "Gespeichert",
  "common.loading": "Laden…",
  "common.error": "Fehler",
  "common.dismiss": "Schließen",
  "common.back": "Zurück",
  "common.cancel": "Abbrechen",
  "common.confirm": "Bestätigen",
  "common.search": "Suchen",
  "common.not_signed_in": "Nicht angemeldet",
  "common.add_account": "Konto hinzufügen",
};

// ---------------------------------------------------------------------------
// Locale registry
// ---------------------------------------------------------------------------

const LOCALES: Record<string, Dict> = {
  "en": EN,
  "en-US": EN,
  "pl": PL,
  "pl-PL": PL,
  "de": DE,
  "de-DE": DE,
};

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

let _locale = "en-US";

export function setLocale(bcp47: string): void {
  _locale = bcp47 || "en-US";
}

export function currentLocale(): string {
  return _locale;
}

/**
 * Translate a key, with optional variable interpolation.
 *
 * `t("instances.subtitle_many", { n: 3 })` → "3 instances · shared libraries and assets"
 */
export function t(key: string, vars?: Record<string, string | number>): string {
  const dict = LOCALES[_locale] ?? LOCALES[_locale.split("-")[0]] ?? EN;
  let value = dict[key] ?? EN[key] ?? key;
  if (vars) {
    for (const [name, replacement] of Object.entries(vars)) {
      value = value.replaceAll(`{${name}}`, String(replacement));
    }
  }
  return value;
}
