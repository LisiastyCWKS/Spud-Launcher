# Changelog

Wszystkie istotne zmiany w projekcie. Format oparty o [Keep a Changelog](https://keepachangelog.com/pl/1.1.0/),
wersjonowanie zgodne z [Semantic Versioning](https://semver.org/lang/pl/).

## [3.0.0] — 2026-11-08

Wydanie skupione na interfejsie: nowa warstwa wizualna, prawdziwy system powiadomień,
potwierdzenia dla akcji nieodwracalnych i pełna lokalizacja powłoki.

### Dodane

- **System toastów** (`lib/toast`, `components/Toaster`) — stos do czterech powiadomień,
  cztery warianty (sukces / info / ostrzeżenie / błąd), akcje w toaście, pauza odliczania
  na hover, pasek czasu, deduplikacja po kluczu i zamykanie gestem.
- **Dialog potwierdzenia** (`lib/confirm`, `components/ConfirmHost`) — promise-based API,
  `alertdialog`, pułapka focusu, Escape i klik w tło anulują, focus startuje na bezpiecznej
  opcji dla akcji destrukcyjnych.
- **Warstwa `v3.css`** — skala elevacji (`--elev-1…4`), tokeny szkła, pierścienie focusu,
  poświata akcentu pod shellem, sticky glass topbar, glass sidebar z wallpaperem.
- **Płynne przełączanie motywu** — `data-theme-switching` na `<html>` daje 320 ms
  cross-fade palety, potem klasa znika, żeby nie spowalniać hoverów.
- **Nowe ikony** — `info`, `alert`, `check-circle`, `chevron-down`.
- **Banner aktualizacji** przepisany bez inline styles, z akcją pobrania i osobnym
  przyciskiem zamknięcia.
- **i18n powłoki** — nazwy widoków, podtytuły, etykiety kont, motywów, potwierdzenia
  i toasty w PL / EN / DE (`lib/i18n.ts`).
- **Pakiety z CurseForge** — wyszukiwarka, panel wyników i obsługa braku klucza API.
- **Sprawdzanie aktualizacji** — `check_update` czyta GitHub Releases i pokazuje banner
  tylko wtedy, gdy nowsza wersja istnieje; błąd sieci jest ignorowany.
- **Discord Rich Presence** — status „Playing Spud Launcher” plus nazwa instancji,
  z konfigurowalnym application ID.
- **Nowa strona projektu** — statyczny landing bez build stepu (HTML + CSS + JS),
  glassmorphism, próbka launchera z zakładkami, galeria i lightbox.

### Zmienione

- Przyciski dostały spójny zestaw mikrointerakcji: uniesienie na hover, ściśnięcie
  na `:active`, pierścień `:focus-visible` i cień zależny od wariantu.
- Karty instancji podnoszą się na hover i pokazują akcentową obwódkę; focus klawiatury
  jest widoczny bez psucia layoutu.
- Pola formularzy reagują na hover i mają miękką poświatę focusu.
- Sidebar poniżej 880 px zwija się do railu ikon (etykiety zostają w `title`), a toast
  stack i dialogi przechodzą w układ jednokolumnowy.
- `InstanceGrid`, `InstanceDetail`, `SettingsPanel` i `AccountsPanel` raportują błędy
  przez jeden kanał: inline + toast, zamiast wyłącznie lokalnego paska.
- `SyncPanel` pyta o usunięcie snapshotu przez nowy dialog.
- `UpdateBanner` używa klas zamiast stylów inline.

### Naprawione

- Usunięte `window.confirm` w czterech miejscach — zablokowany natywny dialog nie
  zatrzymuje już animacji ani nie wygląda obco na tle reszty UI.
- Poprawione mojibake (`â€"`) w komentarzach `app.css`.
- `report(null)` z podpaneli czyściło błąd, ale wcześniej mogło wywołać toast
  „null” — teraz puste wartości są ignorowane.

### Wydanie

- Wersja `3.0.0` w `Cargo.toml`, `src-tauri/tauri.conf.json`, `ui/package.json`
  i `ui/package-lock.json`.
- Nowy `README.md` z badge'ami, podglądem interfejsu, quick startem i spisem treści.
- Landing pokazuje `v3.0.0` jako fallback, zanim API wydań odpowie.

## [0.2.0] — 2026-06-14

### Dodane

- Overlay w grze (drugie okno Tauri, Shift+Tab) z licznikami FPS, TPS i listą modów.
- Pięć palet interfejsu (`spud`, `midnight`, `forest`, `dusk`, `paper`) i edytor akcentu.
- Gęstość, promień narożników, własne tło z regulacją krycia i rozmycia, wyłącznik animacji.
- Wtyczki JavaScript: `plugin.json` + `main.js`, panele, zdarzenia launchera, uprawnienia.
- Synchronizacja w chmurze: snapshoty instancji, przywracanie jako nowa instancja.
- Analiza crashy z lokalnymi regułami i opcjonalnym dostawcą AI (klucz w keyringu OS).
- Strojenie sprzętowe: profil CPU/GPU/RAM i podgląd flag JVM przed zastosowaniem.

### Zmienione

- Wspólny magazyn plików z hardlinkami zamiast kopii per instancja.
- Pełny cykl życia procesu gry: `Running · pid`, blokada drugiej kopii, stop przez `WM_CLOSE`.

## [0.1.0] — 2026-03-02

Pierwsze publiczne wydanie: instancje Vanilla / Fabric / Quilt / Forge / NeoForge,
wspólne biblioteki i assety, logowanie Microsoft (device code) i konta offline,
Modrinth, Java runtime management, import z innych launcherów, eksport `.zip` / `.mrpack`.

[3.0.0]: https://github.com/LisiastyCWKS/Spud-Launcher/compare/v0.2.0...v3.0.0
[0.2.0]: https://github.com/LisiastyCWKS/Spud-Launcher/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/LisiastyCWKS/Spud-Launcher/releases/tag/v0.1.0
