# Spud Launcher

Minimalistyczny, szybki launcher Minecrafta napisany w Rust i Tauri v2 — inspirowany Prism Launcherem i Modrinth App. Każda instancja ma odizolowany katalog `.minecraft`, współdzielone biblioteki i assety trafiają do wspólnego drzewa, a mody, paczki, resource packi i shadery instaluje się jednym kliknięciem z Modrintha.

Projekt jest otwartoźródłowy (MIT) i nastawiony na trzy rzeczy: niski narzut w tle, brak dodatkowych procesów podczas gry i przewidywalny, czytelny kod podzielony na warstwy.

> **Status weryfikacji:** frontend (`ui`) przechodzi `tsc --noEmit` i `vite build`. Backend kompiluje się czysto (`cargo check --workspace`, `cargo build --workspace` bez błędów i ostrzeżeń na Windows 11, Rust 1.99.1), a `cargo test --workspace` przechodzi 37 testów. Sprawdzone end-to-end na żywej sieci i na zbudowanym exe: konto offline (nick → UUID → start legacy), `optimization_install` na Fabric 1.21.1 (10 plików), start gry — Minecraft 26.3 na Fabric 0.20.1 wczytał 64 mody i doszedł do menu — oraz nowy cykl zarządzania procesem: Play zmienia się w **Stop** (`Running · pid`), druga kopia tej samej instancji jest odrzucana, dwie różne instancje startują równolegle, a Stop zamyka grę przez `WM_CLOSE` w 0,65 s i nie rusza drugiej. Usuwanie instancji przechodzi przez `instance_delete`; działającą grę komenda zatrzymuje przed skasowaniem katalogu. Import z innego launchera sprawdzony na profilu Modrinth App („Fabric 26.3”, 32 mody, 55,8 MB): skan odczytał wersję i loader z `logs/latest.log`, bo Modrinth App nie zapisuje żadnego pliku metadanych, a import utworzył instancję z modami, configami, resource packami, shaderami, światami i `options.txt`. Eksport i import sprawdzone w obie strony na tej samej instancji: `.zip` (96 plików, 27,3 MB) wrócił jako instancja z 32 modami, 28 configami, 31 plikami światów, `options.txt` i `servers.dat`, bez `session.lock`; `.mrpack` (27,3 MB) wrócił z metadanymi odczytanymi wyłącznie z `modrinth.index.json`, a paczka z linkiem CDN do Sodium pobrała jar i zgodziła się z SHA-1 co do znaku. Logowanie Microsoft sprawdzone do granicy serwera AAD: `client_id` z Settings trafia do żądania, a zły identyfikator wraca jako `AADSTS700038`, nie `MissingClientId`. Nieprzetestowane pozostają: instalacja Forge/NeoForge, pełne logowanie realnym `client_id`, pobranie Javy 8 przez Adoptium i Rich Presence na prawdziwym `application id`. Stare JSON-y z regułami `disallow` (alpha 1.0) parsują się i instalują poprawnie. Interfejs użytkownika jest na razie w języku angielskim.

---

## Spis treści

- [Funkcje](#funkcje)
- [Stos technologiczny](#stos-technologiczny)
- [Architektura](#architektura)
- [Struktura katalogów](#struktura-katalogów)
- [Wymagania](#wymagania)
- [Budowanie](#budowanie)
- [Konfiguracja logowania Microsoft](#konfiguracja-logowania-microsoft)
- [Tryb offline](#tryb-offline)
- [Pakiety optymalizacyjne](#pakiety-optymalizacyjne)
- [Java](#java)
- [Diagnostyka i konsola](#diagnostyka-i-konsola)
- [Eksport i import instancji](#eksport-i-import-instancji)
- [Import z innych launcherów](#import-z-innych-launcherów)
- [Znajomi](#znajomi)
- [Discord Rich Presence](#discord-rich-presence)
- [Dane launchera](#dane-launchera)
- [Format `instance.json`](#format-instancejson)
- [Format `settings.json`](#format-settingsjson)
- [Tryb portable](#tryb-portable)
- [Komendy Tauri](#komendy-tauri)
- [Znane ograniczenia](#znane-ograniczenia)
- [Licencja](#licencja)

---

## Funkcje

### Instancje

- Pełna izolacja: każda instancja ma własny `.minecraft` (mods, resourcepacks, shaderpacks, saves, config, logs, screenshots).
- Wspólne, niemutowalne pliki gry: `libraries/`, `assets/`, `meta/` — pobierane raz i współdzielone przez wszystkie instancje.
- Identyfikator instancji wyliczany z nazwy (`slugify`), nigdy nie pochodzi wprost od użytkownika — brak trawersji katalogów.
- Jednoklikowa instalacja Vanilla, Fabric, Quilt, Forge i NeoForge.
- Reguły wersji czytane w obu dialektach Mojanga: `action: "allow"` i `action: "disallow"` (stare JSON-y, np. alpha 1.0, używają wyłącznie `disallow`); `deny` jest akceptowane jako alias.
- Osobne argumenty JVM i argumenty gry dla każdej instancji, zmienne środowiskowe, ustawienia okna (rozmiar, fullscreen).
- Automatyczne wykrywanie Javy (`JAVA_HOME`, `PATH`, typowe lokalizacje producentów, katalog zarządzany przez launcher) i wybór runtime: dokładne dopasowanie majora, w drugiej kolejności najmłodsza nowsza wersja, nigdy starsza.
- Rejestr uruchomionych gier — karta instancji i widok szczegółów pokazują **Stop** zamiast **Play**, dopóki proces gry żyje.
- Zatrzymywanie gry w dwóch krokach: najpierw `WM_CLOSE` postawiony bezpośrednio na okno gry (Minecraft zapisuje świat i kończy się sam — zmierzone 0,65 s od menu), a dopiero po 15 sekundach `taskkill /F /T` na całym drzewie procesów. `taskkill /T` bez `/F` nie wystarcza: JVM ma procesy potomne, więc system odmawia ich zamknięcia. Rejestr sam usuwa wpisy po zakończeniu gry — bez odpytywania `tasklist`.
- Druga kopia **tej samej** instancji jest blokowana, ale różne instancje startują równolegle — launcher nie znika po starcie gry.
- Usuwanie instancji z siatki (ikona kosza) i z widoku szczegółów; jeśli gra działa, `instance_delete` najpierw ją zatrzymuje, potem kasuje katalog.
- Tryb `close_on_launch` — po starcie gry launcher minimalizuje okno zamiast kończyć proces, żeby można było odpalić kolejną instancję.

### Pobieranie

- Wielowątkowy downloader (domyślnie 8 równoległych połączeń, konfigurowalne 1–32).
- Wznowienie pobierania przez HTTP `Range` z plików `.part` w `cache/downloads/`.
- Pauza, wznowienie i anulowanie przez `DownloadControl` (flagi atomowe, bez blokowania wątków).
- Weryfikacja SHA-1, SHA-256 i SHA-512; plik trafia na miejsce docelowe tylko po zgodności hasha.
- Wspólny cache zawartości assetów (`assets/objects/`) adresowany SHA-1.

### Modrinth

- Wyszukiwanie modów, modpacków, resource packów i shaderów z filtrami po wersji gry i loaderze.
- Instalacja z automatycznym rozwiązywaniem zależności wymaganych.
- Manifest zainstalowanych modów (`<game>/.spud/mods.json`) z wersją, hashem i typem projektu — pozwala wykryć aktualizacje i bezpiecznie usuwać pliki.
- Jednoklikowa instalacja paczek `.mrpack`: pobranie archiwum, odczyt `modrinth.index.json`, utworzenie instancji, instalacja loadera, pobranie plików paczki i rozpakowanie `overrides/` oraz `client-overrides/`.
- Sprawdzanie aktualizacji zainstalowanych modów.
- **Pakiety optymalizacyjne** — jeden przycisk „Optimize” instaluje dobrany do wersji i loadera zestaw modów wydajnościowych (Sodium/Lithium/Iris na Fabric, Quilt i NeoForge; Embeddium/Oculus na Forge). Projekty bez buildu na tę wersję są raportowane i pomijane, a nie traktowane jako błąd.

### Konta

- Logowanie Microsoft przez OAuth2 **device code flow** — bez lokalnego serwera callback i bez otwierania portu.
- Tokeny trafiają wyłącznie do magazynu systemowego (Windows Credential Manager, macOS Keychain, Secret Service); `accounts.json` zawiera tylko publiczne metadane.
- Wiele kont, wybór aktywnego, usuwanie wraz z wpisem w keyringu.
- **Szybkie przełączanie** — klik w plakietkę konta w stopce sidebara rozwija listę wszystkich kont z zaznaczonym aktywnym. Wybór innego konta woła `account_set_active` i od razu obowiązuje przy następnym starcie gry; nie trzeba się wylogowywać ani wchodzić do panelu Accounts.
- **Tryb offline** — konto z samym nickiem, bez sesji Microsoft: nick 3–16 znaków (litery, cyfry, `_`), UUID liczone jak u vanilla (`OfflinePlayer:<nick>` → MD5 → UUID v3), start z `user_type = "legacy"` i tokenem `"0"`. Żaden sekret nie trafia na dysk, bo żadnego nie ma.

### Java

- Automatyczne wykrywanie Javy (`JAVA_HOME`, `PATH`, rejestr Windows, typowe lokalizacje producentów, katalog zarządzany przez launcher) i wybór runtime: dokładne dopasowanie majora, w drugiej kolejności najmłodsza nowsza wersja, nigdy starsza.
- **Automatyczne pobieranie Javy**, gdy nic zainstalowanego nie pasuje: launcher bierze najnowszy JRE **Eclipse Temurin** z Adoptium, weryfikuje SHA-256 i rozpakowuje do `<root>/java/temurin-<major>/`. Kolejne starty znajdują go bez sieci. Przełącznik `auto_install_java` (domyślnie włączony) obejmuje tylko tryb `auto` — instancja z ręcznie wskazaną ścieżką nigdy nie dociąga Javy po cichu.
- Dwa przyciski w Settings → **Java**: *Install Java 21* i *Install Java 8*, niezależne od ustawień instancji. Ponowne kliknięcie nie pobiera drugi raz — `java_install` najpierw sprawdza dysk.

### Diagnostyka i konsola

- **Konsola na żywo** — launcher ogonuje `<instance>/logs/game.log` (stdout i stderr gry piszą do jednego pliku) i wysyła nowe linie zdarzeniem `game://log`. Odpytywanie co 300 ms, ostatnie 400 linii jako snapshot, bufor widoku trzyma 3000 linii. Tailer startuje razem z `instance_launch` i wyłącza się po 90 s ciszy w pliku.
- Filtry **All / Warnings / Errors**, auto-scroll (Follow), kopiowanie całego widoku jednym klikiem i czyszczenie widoku bez ruszania pliku.
- **Crash Logs** — lista plików z `logs/` i `crash-reports/` (najnowsze pierwsze) z rozmiarem i czasem; klik otwiera podgląd, ikona kopiuje całą treść gotową do wklejenia w zgłoszenie błędu.
- Widok **Diagnostics** w sidebarze zbiera to samo dla dowolnej instancji, a zakładka **Console** w szczegółach instancji pokazuje konsolę bez wychodzenia z kontekstu.

### Eksport i import instancji

- **Eksport `.zip`** — całe `.minecraft` instancji plus `spud-instance.json` z nazwą, wersją i loaderem. Trafia do `<root>/exports/<id>-<data>.zip`, ścieżka ląduje w schowku.
- **Eksport `.mrpack`** — indeks w formacie Modrintha. Mody pochodzące z Modrintha jadą jako linki do CDN (SHA-1 w indeksie), a wszystko pozostałe — configi, światy, `options.txt`, `servers.dat`, resource packi i ręcznie wrzucone jary — jako `overrides/`. Działa też na instancji bez manifestu `.spud/mods.json`: wtedy cała zawartość trafia do `overrides/` i paczka jest nadal poprawnym `.mrpack`.
- **Import** przyjmuje oba formaty. Archiwum jest najpierw rozpakowywane do katalogu tymczasowego, każda ścieżka jest sprawdzana pod kątem wyjścia poza katalog docelowy (`safe_join`), a `session.lock` jest pomijany — import nie może podmienić plików poza instancją ani przenieść cudzego `session.lock`.
- Import `.mrpack` dociąga brakujące jary z listy `files` indeksu i weryfikuje każdy SHA-1, więc paczka z linkami CDN odtwarza się bez ręcznego klikania w Modrinth.
- Import tworzy nową, odizolowaną instancję. Pliki gry dociąga się potem zwykłym **Install** — archiwum nie zawiera client jara ani bibliotek.

### Interfejs

- React 18 + TypeScript, bez biblioteki animacji — cała warstwa ruchu to CSS.
- Ruch trzyma się jednego rytmu: tokeny czasu i krzywych (`--dur-1…4`, `--ease-out`, `--ease-spring`, `--ease-in-out`) w `theme.css`, animacje w jednej sekcji `app.css`. Karty instancji, wyniki Modrinth i wiersze kont wchodzą kaskadą (`--i` → `animation-delay`), zmiana widoku odtwarza wejście dzięki kluczowanemu wrapperowi `.view`, a ładowanie zamiast pojedynczego spinnera pokazuje karty-szkielety z połyskiem. `prefers-reduced-motion: reduce` wyłącza całą warstwę jednym regułem w `theme.css`.
- Motywy ciemny, jasny i systemowy, przełączane bez przeładowania; preferencja zapamiętywana lokalnie (`localStorage`, klucz `spud.theme`).
- **Własny kolor akcentu** — paleta ośmiu presetów plus dowolny kolor z natywnego pickera albo pola hex i suwaków HSL. Z jednego odcienia liczone są cztery zmienne motywu (`--accent`, `--accent-strong`, `--accent-ink`, `--focus`); jasność jest przycinana osobno dla motywu ciemnego (52–74 %) i jasnego (26–40 %), a kolor tekstu na akcencie wybiera kontrast WCAG, nie zgadywanie. Kolor trzymany jest w `localStorage` (klucz `spud.accent`), więc działa per maszyna i wraca po restarcie; „Reset to spud green” przywraca domyślną zieleń.
- **Pełny ekran** — `F11` przełącza, `Esc` wychodzi (o ile nie jest otwarte okno dialogowe). Start gry najpierw zdejmuje pełny ekran, a potem — jeśli włączysz `close_on_launch` — minimalizuje launcher; po wyjściu z gry okno wraca i dostaje fokus (`restore_on_exit`). Ikona w pasku górnym pokazuje aktualny stan, zdarzenie `window://fullscreen` trzyma UI w zgodzie z backendem.
- Lista instancji, widok szczegółów, kreator nowej instancji, panel Modrinth, panel kont, ustawienia.
- Sidebar w dwóch grupach: **Library** (Instances, Discover, Import, Friends, Diagnostics) i **You** (Accounts, Settings) — stopka trzyma przełącznik motywu i plakietkę konta z szybkim przełączaniem.
- Postęp pobierania jako pojedynczy toast aktualizowany zdarzeniami z backendu (bez odpytywania).
- Natywne okna wyboru pliku i katalogu (`tauri-plugin-dialog`) przy imporcie archiwum i wskazywaniu folderu współdzielonego dla znajomych.

---

## Stos technologiczny

| Warstwa | Technologia |
|---|---|
| Rdzeń | Rust 2021, `rust-version = 1.77` |
| Powłoka desktopowa | Tauri v2 |
| Frontend | React 18.3, TypeScript 5.5, Vite 5.4 |
| HTTP | `reqwest` (rustls, gzip, stream) |
| Async | `tokio`, `futures` |
| Hashowanie | `sha1`, `sha2`, `hex` |
| Archiwa | `zip` (deflate) |
| Sekrety | `keyring` 3 |
| Serializacja | `serde`, `serde_json`, `chrono` |

Profil release: `opt-level = "s"`, `lto = "thin"`, `codegen-units = 1`, `strip = true`, `panic = "abort"` — mały binarny plik i niski narzut pamięci.

---

## Architektura

Kod jest podzielony na warstwy o jednokierunkowych zależnościach. Warstwa domenowa nie wie nic o HTTP, Tauri, keyringu ani UI.

```
launcher-core   ← czysta domena: ścieżki, konfiguracja, Java, metadane wersji, plan startu
     ↑
launcher-net    ← pobieranie, cache, instalacja gry i loaderów, .mrpack
launcher-auth   ← OAuth2 device code, magazyn kont i tokenów
launcher-modrinth ← klient API Modrintha, manifest zainstalowanych modów
     ↑
src-tauri       ← komendy Tauri, stan aplikacji, most do webview
     ↑
ui              ← React + TypeScript, wyłącznie przez `invoke`/`listen`
```

Zasady, które trzymają tę strukturę:

- `launcher-core` nie ma zależności od `reqwest` ani `tauri` — można go testować bez sieci i bez GUI.
- Komendy Tauri są cienkie: walidacja wejścia, wywołanie serwisu, mapowanie błędu na `String`.
- Stan współdzielony (`AppState`) trzyma klienty HTTP, downloader, installer, loader service, auth, konta i klienta Modrintha. Blokada ustawień jest zdejmowana przed każdym `await`.
- Każdy element UI ma odpowiednik w `ui/src/lib/api.ts`; nazwy komend muszą być zgodne z `src-tauri/src/lib.rs`.

### Przepływ instalacji

1. `instance_install` ładuje `instance.json`.
2. `GameInstaller::install_vanilla` pobiera manifest Mojang, JSON wersji, client jar, biblioteki, log config i assety — z pominięciem plików już zweryfikowanych.
3. Dla instancji modowanej wybierany jest runtime Javy spełniający wymagania wersji, a następnie instalowany profil loadera:
   - Fabric/Quilt: profile JSON z meta API, biblioteki do wspólnego drzewa.
   - Forge/NeoForge: oficjalny installer jar uruchamiany w katalogu staging, następnie JSON wersji i biblioteki kopiowane do `meta/` i `libraries/`.
4. `GameInstaller::ensure_version_files` przechodzi scalony profil i dociąga każdą brakującą bibliotekę. To domyka lukę, której `install_vanilla` nie widzi: profile Fabric/Quilt deklarują własne biblioteki (ASM, Sponge Mixin, sam loader) z Maven Fabric, a instalator Forge potrafi zostawić ogony. Bez tego kroku start kończył się błędem `N file(s) missing`.
5. `game.version_id` zostaje zapisany w konfiguracji — bez niego start jest odmawiany.

Ten sam mechanizm naprawczy działa przy każdym starcie: `instance_launch` wywołuje `ensure_version_files` przed zbudowaniem planu, więc skasowana biblioteka loadera dociąga się sama, bez klikania Repair.

### Przepływ startu

1. `instance_launch` scala JSON wersji vanilla i loadera (`merged_for_instance`).
2. Wybiera Javę, pobiera aktywną sesję konta i token z keyringu.
3. `build_plan` składa pełną linię poleceń: flagi JVM, `-Djava.library.path` do natives, classpath, argumenty gry z podstawieniami `${...}`.
4. Proces gry startuje w katalogu instancji, log trafia do pliku w `logs/`.
5. PID i uchwyt `Child` trafiają do `RunningGames`, a zdarzenie `game://changed` odświeża UI — przycisk zmienia się na **Stop**.
6. Przy `close_on_launch` launcher minimalizuje okno 600 ms po spawnie; proces żyje dalej i trzyma uchwyt do gry.

---

## Struktura katalogów

```
spud-launcher/
├── Cargo.toml                     # workspace, wspólne wersje zależności, profil release
├── rust-toolchain.toml
├── LICENSE
├── README.md
├── examples/
│   ├── settings.json
│   └── instances/
│       ├── vanilla-1-21-1/instance.json
│       ├── fabric-skyblock/instance.json
│       └── neoforge-tech/instance.json
├── crates/
│   ├── launcher-core/
│   │   └── src/
│   │       ├── error.rs           # CoreError, atomowy zapis JSON, kontekst IO
│   │       ├── paths.rs           # układ katalogów, slugify, tryb portable
│   │       ├── instance.rs        # InstanceConfig, NewInstance, walidacja id, safe_join
│   │       ├── settings.rs        # LauncherSettings, Theme, sanityzacja zakresów
│   │       ├── java.rs            # wykrywanie JVM, parsowanie wersji, wybór runtime
│   │       ├── version.rs         # JSON wersji, reguły OS/features, biblioteki, assety
│   │       └── launch.rs          # scalanie wersji, plan startu, podstawienia argumentów
│   ├── launcher-net/
│   │   └── src/
│   │       ├── download.rs        # DownloadSpec, DownloadManager, Range, hash, progres
│   │       ├── cache.rs           # bezpieczne nazwy plików
│   │       ├── install.rs         # GameInstaller: vanilla + profile loaderów
│   │       ├── loaders.rs         # meta API Fabric/Quilt, listy buildów
│   │       └── mrpack.rs          # odczyt i instalacja .mrpack
│   ├── launcher-auth/
│   │   └── src/
│   │       ├── microsoft.rs       # device code, polling, Xbox Live, profil Minecraft
│   │       └── store.rs           # AccountStore (accounts.json) + TokenVault (keyring)
│   └── launcher-modrinth/
│       └── src/
│           ├── client.rs          # search, project, versions, zależności
│           └── installed.rs       # manifest .spud/mods.json
├── src-tauri/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs                 # rejestracja wszystkich komend
│       ├── state.rs               # AppState::bootstrap
│       ├── running.rs             # rejestr uruchomionych gier
│       ├── discord.rs             # Rich Presence po nazwanym potoku Discord IPC
│       ├── java_install.rs        # pobieranie JRE z Adoptium + SHA-256
│       └── commands/
│           ├── instances.rs
│           ├── game.rs
│           ├── mods.rs
│           ├── accounts.rs
│           ├── system.rs
│           ├── console.rs         # tailer logów + game://log
│           ├── transfer.rs        # eksport/import .zip i .mrpack
│           ├── launchers.rs       # wykrywanie i import z innych launcherów
│           └── friends.rs         # presence w folderze współdzielonym
└── ui/
    ├── package.json
    ├── vite.config.ts
    ├── tsconfig.json
    ├── index.html
    └── src/
        ├── main.tsx
        ├── App.tsx
        ├── hooks/useTheme.ts
        ├── lib/api.ts             # wrappers na invoke/listen
        ├── lib/types.ts           # lustro typów z Rusta
        ├── lib/format.ts
        ├── styles/theme.css
        ├── styles/app.css
        └── components/
            ├── Sidebar.tsx
            ├── InstanceGrid.tsx
            ├── InstanceDetail.tsx
            ├── CreateInstanceDialog.tsx
            ├── ModrinthPanel.tsx
            ├── AccountsPanel.tsx
            ├── SettingsPanel.tsx
            ├── ConsolePanel.tsx
            ├── DiagnosticsPanel.tsx
            ├── ImportPanel.tsx
            ├── FriendsPanel.tsx
            ├── ProgressToast.tsx
            ├── Modal.tsx
            └── Icon.tsx
```

---

## Wymagania

### Wspólne

- Node.js 18 lub nowszy (zalecany 20 LTS) i npm.
- Rust 1.77 lub nowszy, instalowany przez [rustup](https://rustup.rs/).
- Tauri CLI v2 (`cargo install tauri-cli --version "^2"` lub `npx @tauri-apps/cli@^2`).

### Windows

- Microsoft C++ Build Tools z pakietem „Desktop development with C++” (MSVC + Windows SDK).
- WebView2 Runtime — w Windows 10/11 zwykle już obecny.

### macOS

- Xcode Command Line Tools: `xcode-select --install`.

### Linux

- Biblioteki systemowe Tauri v2, np. dla Debiana/Ubuntu:

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

- Dla keyringu: działający Secret Service (GNOME Keyring, KWallet) lub `gnome-keyring`/`dbus` w sesji.

---

## Budowanie

### 1. Zależności frontendu

```bash
npm --prefix ui install
```

### 2. Weryfikacja typów i build UI

```bash
npm --prefix ui run build
```

Wynik trafia do `ui/dist` (ok. 194 kB JS, 60 kB po gzipie, 15 kB CSS). `tauri.conf.json` wskazuje ten katalog jako `frontendDist`.

### 3. Kompilacja backendu

```bash
cargo build --features custom-protocol
```

lub wersja release:

```bash
cargo build --release --features custom-protocol
```

Flaga `custom-protocol` decyduje, czy webview serwuje wbudowany `ui/dist` (build produkcyjny), czy `devUrl` z `localhost:5173` (build deweloperski). Bez niej powstały exe działa tylko przy uruchomionym Vite.

### 4. Tryb deweloperski

Tauri CLI bez instalowania globalnego (zalecane, pobiera gotowy binarny CLI):

```powershell
npm.cmd exec --yes --package="@tauri-apps/cli@^2" -- tauri dev
```

Albo klasycznie, po `cargo install tauri-cli --version "^2" --locked`:

```bash
cargo tauri dev
```

Tauri uruchamia `beforeDevCommand` z `src-tauri/tauri.conf.json` w katalogu `ui/` (`npm run dev`), stawia Vite na `http://localhost:5173` i podłącza webview do dev serwera. `cargo` musi być w `PATH` — w świeżym terminalu jest, w starym dorzuć `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`.

Bezpośredni start zbudowanego pliku (bez dev serwera, frontend wbudowany z `ui/dist`):

```powershell
cargo build --features custom-protocol
.\target\debug\spud-launcher.exe
```

Sama flaga `--features custom-protocol` jest obowiązkowa: bez niej Tauri kompiluje build deweloperski i webview ładuje `devUrl` (`localhost:5173`), więc uruchomiony z Explorera exe pokaże `ERR_CONNECTION_REFUSED`. `tauri build` włącza tę flagę sam.

### 5. Paczka instalacyjna

```powershell
npm.cmd exec --yes --package="@tauri-apps/cli@^2" -- tauri build
```

lub `cargo tauri build` przy lokalnym CLI. `beforeBuildCommand` sam przebudowuje `ui/dist`, więc nie trzeba pamiętać o `npm --prefix ui run build`. Ikony w `src-tauri/icons/` są już wygenerowane (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico`, `icon.icns`).

Wynik na Windows x64 (`targets: "all"`, kategoria `Game`):

| Plik | Rozmiar | Przeznaczenie |
|---|---|---|
| `target/release/bundle/nsis/Spud Launcher_0.1.0_x64-setup.exe` | ~2,4 MiB | instalator dla użytkownika |
| `target/release/bundle/msi/Spud Launcher_0.1.0_x64_en-US.msi` | ~3,4 MiB | wdrożenie przez GPO / `msiexec` |
| `target/release/spud-launcher.exe` | ~7,3 MiB | wersja portable, bez instalacji |

NSIS instaluje per-user do `%LOCALAPPDATA%\Spud Launcher`, zakłada skrót na pulpicie i w menu Start, dopisuje wpis do „Aplikacje i funkcje” z własnym `uninstall.exe`. Cicha instalacja i deinstalacja: `setup.exe /S`, `uninstall.exe /S`. Setup dociąga WebView2 tylko wtedy, gdy system go nie ma (Windows 11 ma go fabrycznie).

**Przed publikacją:**

1. Podnieś wersję w dwóch miejscach — `version` w `src-tauri/tauri.conf.json` i `[workspace.package] version` w `Cargo.toml`. Tauri bierze numer do nazwy pliku i wpisu w rejestrze.
2. Przepuść `cargo test --workspace` i `npm --prefix ui run build` (to drugie robi się samo w `tauri build`, ale `tsc` warto mieć zielone osobno).
3. Policz sumy i wrzuć je do notatki wydania:

```powershell
Get-FileHash "target\release\bundle\nsis\Spud Launcher_0.1.0_x64-setup.exe" -Algorithm SHA256
```

**Podpis cyfrowy.** Bundel wychodzi niepodpisany, więc SmartScreen pokaże „Windows protected your PC”, dopóki plik nie zbierze reputacji. Podpisuje się po zbudowaniu:

```powershell
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /a "target\release\bundle\nsis\Spud Launcher_0.1.0_x64-setup.exe"
```

Certyfikat OV/EV (Certum, Sectigo, SSL.com) jest płatny; EV zdejmuje ostrzeżenie od razu, OV po pewnej liczbie pobrań. Wariant bez certyfikatu: publikuj `target\release\spud-launcher.exe` jako portable i zaznacz w notatce wydania, że SmartScreen trzeba obejść przez „More info → Run anyway”.

### Testy jednostkowe

```bash
cargo test
```

Testy obejmują m.in. `slugify`, odrzucanie trawersji w `safe_join`, parsowanie wersji Javy, wybór runtime i reguły wersji.

---

## Konfiguracja logowania Microsoft

Launcher używa publicznego klienta OAuth2 i **device code flow**. Własny `client_id` jest wymagany — bez niego `account_begin_login` zwraca `MissingClientId`.

1. Wejdź na [Azure Portal → App registrations](https://portal.azure.com/#view/Microsoft_AAD_IAM/ActiveDirectoryMenuBlade/~/RegisteredApps) i utwórz nową rejestrację.
2. Typ kont: **Accounts in any organizational directory and personal Microsoft accounts** (aplikacja używa endpointu `/consumers`).
3. W sekcji **Authentication** włącz **Allow public client flows** („Enable the following mobile and desktop flows”). Nie dodawaj adresu przekierowania — device code go nie używa.
4. Skopiuj **Application (client) ID**.
5. W launcherze: **Settings → Microsoft sign-in** → wklej ID → **Save**. Pole trafia do `settings.json` jako `ms_client_id`; wyczyszczenie pola usuwa wartość.

Kolejność rozwiązywania ID przy rozpoczęciu logowania:

1. zmienna środowiskowa `SPUD_MS_CLIENT_ID` — nadpisuje ustawienia, wygodna przy `tauri dev`,
2. `ms_client_id` z `settings.json`,
3. brak — `account_begin_login` zwraca `MissingClientId`, a panel **Accounts** pokazuje ostrzeżenie z odesłaniem do Settings.

Komenda `auth_status` zwraca `{ configured, source, client_id }`, gdzie `source` to `env`, `settings` albo `none`. `AppState` przebudowuje klienta auth po każdym `settings_save`, więc nowe ID działa bez restartu aplikacji.

```powershell
# Windows (PowerShell, bieżąca sesja) — nadpisanie na czas developmentu
$env:SPUD_MS_CLIENT_ID = "00000000-0000-0000-0000-000000000000"
npm.cmd exec --yes --package="@tauri-apps/cli@^2" -- tauri dev
```

Zakresy: `XboxLive.signin offline_access`. `offline_access` daje refresh token, więc logowanie przeżywa restart aplikacji. Tokeny są zapisywane w keyringu pod usługą `spud-launcher`, klucz to UUID konta.

---

## Tryb offline

Konto offline to sam nick — bez Microsoftu, bez keyringu, bez sieci przy starcie.

**Dodawanie (UI):** *Accounts* → sekcja **Offline account** → wpisz nick → **Add offline**. Konto od razu staje się aktywne i dostaje plakietkę `Offline`.

**Dodawanie (komenda):**

```ts
const view = await invoke("account_add_offline", { username: "siurek" });
```

**Zasady nicku** — te same co u vanilla: 3–16 znaków, wyłącznie ASCII litery, cyfry i `_`. Walidacja siedzi w `launcher_auth::validate_offline_name`; naruszenie zwraca `AuthError::OfflineName`.

**UUID** liczone jest dokładnie tak, jak robi to serwer w trybie offline:

```
md5("OfflinePlayer:<nick>") → wersja 3 (bajt 6) + wariant RFC 4122 (bajt 8) → UUID z myślnikami
```

Ten sam nick daje ten sam UUID na każdej maszynie, więc światy single-player i serwery offline widzą tę samą tożsamość.

**Start gry** (`game.rs`): `access_token = "0"`, `user_type = "legacy"`, `xuid = null`. Klient nie kontaktuje się z sesją Microsoftu.

**Czego nie ma:** skinów z konta Microsoft (fallback na domyślny), Realms i serwery online odrzucą logowanie. Instalacja, mody, single-player działają normalnie.

`accounts.json` przechowuje `kind: "offline"`; stare wpisy bez tego pola są czytane jako `microsoft` dzięki `#[serde(default)]`.

---

## Pakiety optymalizacyjne

`optimization_install` instaluje cały zestaw modów wydajnościowych dobrany do loadera instancji. Listy są kuratorowane w `src-tauri/src/commands/mods.rs` jako slugi Modrinth:

| Loader | Zestaw |
|---|---|
| Fabric, Quilt | `sodium`, `lithium`, `ferrite-core`, `krypton`, `immediatelyfast`, `entityculling`, `modernfix`, `sodium-extra`, `iris` |
| Forge | `embeddium`, `oculus`, `ferrite-core`, `modernfix`, `entityculling`, `canary` |
| NeoForge | `sodium`, `lithium`, `ferrite-core`, `modernfix`, `entityculling`, `immediatelyfast`, `iris` |
| Vanilla | brak — komenda zwraca błąd i podpowiada instalację loadera |

Uwaga na Forge: od Minecrafta 1.20.2 większość modów wydajnościowych (Embeddium, Oculus, ModernFix) przeniosła się na NeoForge. Na Forge 1.21+ zestaw zwróci same pozycje `skipped` i błąd z podpowiedzią „try a NeoForge instance”. Zestaw Forge ma sens przede wszystkim na 1.20.1.

Dla każdego sluga komenda bierze najnowszy build pasujący do `game.version` i loadera, dociąga zależności przez `resolve_dependencies` i pomija duplikaty po `project_id`. Projekt bez buildu na tę wersję **nie przerywa** operacji — ląduje w `skipped` z powodem.

```ts
interface OptimizationResult {
  installed: InstalledMod[];
  skipped: string[];
}
```

**UI:** zakładka *Mods* instancji z loaderem → przycisk **Optimize** obok *Check updates*. Po instalacji lista pokazuje zainstalowane mody i pominięte projekty. Pliki trafiają do `<game>/.minecraft/mods` i do manifestu `.spud/mods.json`, więc *Check updates* widzi je później jak każdy inny mod.

Pobieranie idzie tym samym potokiem co `mods_install` — SHA-1/SHA-512, współdzielony cache, postęp na kanale `mods://progress`.

---

## Java

Launcher nie wymaga Javy w systemie. Runtime jest wybierany w trzech krokach:

1. **Tryb instancji.** `java.mode = "manual"` + `java.path` — używane bez żadnych decyzji. `java.mode = "auto"` idzie dalej.
2. **Wykrywanie.** `java::discover` przeczesuje `JAVA_HOME`, `PATH`, rejestr Windows (klucz `JDK`/`JRE`), typowe katalogi producentów (Adoptium, Oracle, Zulu, Corretto, Microsoft, Liberica) i `<root>/java/`. Dla każdego kandydata uruchamiane jest `java -XshowSettings:properties -version`, z którego parsowane są `java.version`, `java.vendor` i `os.arch`.
3. **Wybór.** Dokładne dopasowanie majora wygrywa. Bez trafienia launcher bierze najmłodszą nowszą wersję — nigdy starszą. Wymagany major liczy `java::guess_major` z wersji gry (8 dla ≤ 1.16 i starych alph, 17 dla 1.17–1.20.4, 21 dla 1.20.5+).

Gdy nic nie pasuje, a `auto_install_java` jest włączone, wchodzi `java_install`:

```
GET https://api.adoptium.net/v3/assets/latest/{major}/hotspot
    ?architecture={x64|aarch64}&image_type=jre&os={windows|mac|linux}&vendor=eclipse
```

Z odpowiedzi brane jest `binary.package.link` i `binary.package.checksum`. Plik ląduje w `cache/downloads/`, SHA-256 jest liczony w locie i porównywany z sumą Adoptium — niezgodność przerywa instalację i nic nie trafia na miejsce docelowe. Archiwum jest rozpakowywane do `<root>/java/temurin-<major>/`, a `java::discover` znajduje je przy kolejnym starcie jak każdy inny runtime.

Cały katalog `<root>/java/` jest zarządzany przez launcher: usunięcie go jest bezpieczne (kolejny start po prostu pobierze Javę jeszcze raz), a nic nie jest zapisywane do `PATH`, rejestru ani katalogów systemowych.

**UI:** Settings → *Java* → przełącznik *Download a matching Temurin JRE automatically* oraz przyciski *Install Java 21* / *Install Java 8*. `java_install` najpierw woła `java_install::installed()`, więc powtórne kliknięcie odpowiada natychmiast z `bytes: 0`.

---

## Diagnostyka i konsola

Konsola czyta plik, nie potok. `instance_launch` przekierowuje stdout i stderr gry do `<instance>/logs/game.log` i od razu startuje tailer — jeden na instancję, trzymany w `AppState.logs`.

| Parametr | Wartość | Dlaczego |
|---|---|---|
| `POLL_INTERVAL` | 300 ms | tyle wystarcza, żeby konsola wyglądała na żywo, a odczyt pliku był darmowy |
| `TAIL_LINES` | 400 | snapshot po wejściu w widok — dość, żeby zobaczyć kontekst ostatniego błędu |
| `IDLE_TIMEOUT` | 90 s | plik, który przestał rosnąć, zwalnia wątek; kolejny start podnosi tailer od nowa |

Nowe linie lecą zdarzeniem `game://log` w payloadzie `{ instance_id, lines }`. UI skleja je w bufor 3000 linii, filtruje po `ERROR`/`FATAL`/`SEVERE` i `WARN`/`WARNING` i przewija do końca, dopóki *Following* jest włączone.

**Crash logs.** `instance_log_files` listuje `logs/*.log` i `crash-reports/*.txt` (najnowsze pierwsze, z rozmiarem i czasem modyfikacji). `instance_log_read` przyjmuje tylko ścieżki wewnątrz tych dwóch katalogów — `safe_join` blokuje wyjście poza katalog instancji, więc komenda nie może posłużyć do czytania dowolnych plików. Obie akcje — otwarcie i kopiowanie — są w panelu, bo tragedia z modem kończy się zwykle wklejeniem logu do zgłoszenia.

```ts
const snapshot = await invoke("instance_logs", { id: "fabric-skyblock" });
const files = await invoke("instance_log_files", { id: "fabric-skyblock" });
const text = await invoke("instance_log_read", { id: "fabric-skyblock", name: "crash-reports/crash-2026-10-30_21.14.02-client.txt" });
```

---

## Eksport i import instancji

`instance_export` obsługuje dwa formaty. Oba kończą w `<root>/exports/` z nazwą `<id>-<YYYYMMDD-HHMMSS>.<format>`, a UI kopiuje ścieżkę do schowka.

**`zip`** — pełne `.minecraft` instancji (mody, configi, resource packi, shadery, światy, screenshots, `options.txt`, `servers.dat`) plus `spud-instance.json` z nazwą, wersją gry, loaderem i wersją loadera. Przenosi wszystko, waży tyle, ile instancja.

**`mrpack`** — indeks w formacie Modrintha. Mody z manifestu `.spud/mods.json`, które mają `project_id`, `version_id` i znany SHA-1, dostają URL z CDN Modrintha — paczka waży wtedy tyle, co same configi. Wszystko pozostałe idzie jako `overrides/`: `config/`, `defaultconfigs/`, światy, screenshots, resource packi, shadery, `options.txt`, `servers.dat` i każdy jar, którego Modrinth nie wydał. Logi, `crash-reports/`, `session.lock` i `.spud/` są wykluczone — to dane robocze, nie zawartość paczki. Instancja bez manifestu (np. zaimportowana z innego launchera) też się eksportuje: indeks wychodzi z pustą listą `files`, a cała zawartość ląduje w `overrides/`.

Klucze loaderów w indeksie to nazwy ze specyfikacji Modrintha — `fabric-loader`, `quilt-loader`, `forge`, `neoforge` — a nie skróty używane wewnątrz launchera, bo pod skrótem inne launchery loadera nie rozpoznają.

`instance_import` przyjmuje `.zip` i `.mrpack`:

1. Archiwum jest rozpakowywane do katalogu tymczasowego (`tempfile`), na dysk docelowy nic jeszcze nie trafia.
2. Każda ścieżka przechodzi przez `safe_join` — wpis `../../evil` jest odrzucany, nie „naprawiany”.
3. `session.lock` jest pomijany: to blokada żywej sesji, nie zawartość paczki.
4. Instancja powstaje przez `InstanceConfig::create`, więc dostaje własny slug i własne `.minecraft`. Nazwa jest de-duplikowana, jeśli taka już istnieje.
5. W `.mrpack` brakujące jary z `files` indeksu są dociągane z CDN i weryfikowane po SHA-1 — paczka z linkami odtwarza komplet modów. Pliki już obecne w `overrides/` nie są pobierane drugi raz.
6. Pliki gry (client jar, biblioteki, assety) dociąga się przyciskiem **Install** — archiwum ich nie zawiera.

Gdy w archiwum nie ma `spud-instance.json`, nazwa, wersja gry i loader biorą się z `modrinth.index.json` (`name`, `dependencies.minecraft`, `dependencies["fabric-loader"]` itd.), a w ostateczności z nazwy pliku. Oba pliki JSON są czytane z tolerancją BOM — paczka zapisana Notatnikiem na Windowsie ma na początku `EF BB BF` i bez tego `serde_json` jej nie przyjmie.

---

## Import z innych launcherów

`launchers_scan` przegląda znane lokalizacje i zwraca listę profili z nazwą launchera, ścieżką, wersją gry, loaderem, liczbą modów i rozmiarem. Skan jest wyłącznie do odczytu — nic nie jest przenoszone ani modyfikowane.

| Launcher | Lokalizacja | Skąd bierze się wersja i loader |
|---|---|---|
| Prism Launcher | `%APPDATA%\PrismLauncher\instances` | `mmc-pack.json` (komponenty `net.minecraft`, `net.fabricmc.fabric-loader`, `org.quiltmc.quilt-loader`, `net.minecraftforge`, `net.neoforged`) + `instance.cfg` dla nazwy |
| MultiMC / PolyMC | `%APPDATA%\MultiMC\instances`, `%APPDATA%\PolyMC\instances` | ten sam format co Prism |
| Modrinth App | `%APPDATA%\com.modrinth.theseus\profiles`, `%LOCALAPPDATA%\...`, `%APPDATA%\ModrinthApp\profiles` | `profile.json` jeśli jest; w przeciwnym razie nagłówek `logs/latest.log` |
| CurseForge | `~/curseforge/minecraft/Instances`, `~/Documents/curseforge/minecraft/Instances` | `minecraftinstance.json` (`gameVersion`, `baseModLoader`) |
| ATLauncher | `%APPDATA%\ATLauncher\instances` | `instance.json` (`minecraftVersion`, `loader.type`) |
| GDLauncher | `%APPDATA%\gdlauncher_next\instances` | `config.json` |
| Technic | `%APPDATA%\.technic\modpacks` | brak metadanych — tylko nazwa folderu i zawartość |
| Lunar Client | `~/.lunarclient/profiles` | jeden katalog na wersję gry, nazwa katalogu jest wersją |
| Minecraft Launcher | `%APPDATA%\.minecraft` | `versions/<id>` jeśli log nie mówi nic |

**Fallback z logu.** Modrinth App nie zapisuje w profilu żadnego pliku z wersją — katalog zawiera tylko `.minecraft`. Dlatego `detect_from_log` czyta pierwsze ~60 linii `logs/latest.log` i `logs/game.log` w poszukiwaniu linii, którą Minecraft wypisuje przy każdym starcie:

```
[10:21:06] [main/INFO]: Loading Minecraft 26.1 with Fabric Loader 0.19.5
```

Z niej brane są wersja gry, rodzaj loadera i jego wersja. Gdy i tego nie ma, `version_from_folder` bierze najnowszy katalog w `versions/`. Profil bez żadnej z tych wskazówek trafia na listę z pustym polem wersji — trzeba ją wpisać ręcznie przed importem, bo instancja bez `mc_version` nie ma czego instalować.

**`launcher_import`** kopiuje `mods/`, `config/`, `defaultconfigs/`, `resourcepacks/`, `shaderpacks/`, `saves/`, `screenshots/` oraz `options.txt` i `servers.dat` do nowej, odizolowanej instancji. Symlinki są pomijane, więc import nie przeciągnie plików spoza profilu. Światy idą razem z modami, bo „przenieś mi paczkę” w praktyce znaczy też „moje buildy”. Pliki gry dociąga **Install**.

---

## Znajomi

Znajomi nie mają serwera. Obecność to plik JSON, który każdy launcher zapisuje w folderze wskazanym w **Friends → Shared folder** — OneDrive, Dropbox, Syncthing, udział sieciowy, cokolwiek, co widzą obie strony.

```
<shared>/
├── olivia.json            # pisze launcher olivii
├── lisek213.json          # pisze launcher liska
└── archives/
    ├── olivia__fabric-26-3.zip
    └── lisek213__pvp-pack.zip
```

**Publikowanie.** `friends_publish` wołane jest po zapisie ustawień, po starcie i po wyjściu z gry, a dodatkowo wątek `spawn_publisher` powtarza je co 45 s. Plik zawiera nick, wersję launchera, znacznik czasu, status (`playing`/`idle`), nazwę instancji, wersję gry, loader, adres serwera i listę instancji.

**Serwer.** `server_from_log` czyta ostatnie 256 kB `logs/game.log` i szuka ostatniej linii `Connecting to <host>`, kasując wynik przy `Stopping!` i `Disconnecting`. To ta sama linia, którą Minecraft wypisuje przy każdym wejściu na serwer, więc nie trzeba niczego sniffować.

**Odczyt.** `friends_list` czyta `<nick>.json` dla każdego nicku z listy i liczy `online` jako „plik zapisany w ciągu ostatnich 5 minut”. Brak pliku albo stary znacznik to po prostu `offline` — nie ma odpytywania nikogo przez sieć.

**Kopiowanie instancji.** `friends_share_instance` eksportuje wybraną instancję jako zwykły zip do `archives/<nick>__<id>.zip`, a `friends_import` robi dokładnie to samo, co import archiwum: rozpakowanie do katalogu tymczasowego, kontrola ścieżek, nowa instancja.

```json
{
  "nick": "olivia",
  "app": "spud-launcher/0.1.0",
  "updated_at": "2026-10-30T21:14:02.113Z",
  "status": "playing",
  "server": "play.example.com",
  "instance": "Fabric 26.3",
  "mc_version": "26.3",
  "loader": "fabric",
  "instances": [
    { "id": "fabric-26-3", "name": "Fabric 26.3", "mc_version": "26.3", "loader": "fabric", "loader_version": "0.19.5", "mods": 32 }
  ]
}
```

Konfiguracja lokalna leży w `<root>/friends.json` (`nick`, `sync_folder`, `friends`). Nick nie może zawierać znaków systemowych plików — jest nazwą pliku.

---

## Discord Rich Presence

Launcher mówi do lokalnego klienta Discorda po nazwanym potoku (`\\.\pipe\discord-ipc-0` … `-9`), własnym klientem IPC — bez zależności, bez wtyczki, bez wysyłania czegokolwiek do internetu. Ramka to 4 bajty długości (little-endian) i JSON.

Wątek presence startuje razem z aplikacją i pracuje w tle: łączy się, wysyła `SET_ACTIVITY`, a gdy Discord jest zamknięty, próbuje ponownie co 20 s. Utracone połączenie wykrywane jest po błędzie zapisu — potok nie przyjmuje już bajtów, więc worker zamyka go i wraca do pętli łączenia.

**Wymagany `application id`.** Discord przypisuje aktywność do aplikacji, więc sam przełącznik nie wystarcza. Wejdź na [discord.com/developers](https://discord.com/developers/applications) → *New Application* → skopiuj **Application ID** → Settings → *Discord* → wklej w *Discord application ID*. Bez identyfikatora worker nie próbuje się nawet łączyć.

| Pole aktywności | Źródło |
|---|---|
| `details` | `Playing <nazwa instancji>` w grze, `In the launcher` poza nią |
| `state` | `<loader> · <wersja gry>` w grze, `Picking an instance` poza nią |
| `assets.large_text` | `Spud Launcher` |
| `timestamps.start` | moment ustawienia aktywności (`playing`) |

Wartości `details` i `state` są przycinane do 128 znaków — Discord odrzuca dłuższe i po cichu nie pokazuje nic.

Włączenie i wyłączenie działa natychmiast — `settings_save` przebudowuje worker, więc nie trzeba restartować launchera.

---

## Dane launchera

Domyślna lokalizacja (przez `directories::ProjectDirs`, kwalifikator `dev`, organizacja `spud`, nazwa `SpudLauncher`):

| System | Ścieżka |
|---|---|
| Windows | `%APPDATA%\spud\SpudLauncher` |
| macOS | `~/Library/Application Support/dev.spud.SpudLauncher` |
| Linux | `~/.local/share/spudlauncher` |

Układ wewnętrzny:

```
<root>/
├── settings.json
├── accounts.json              # publiczne metadane kont
├── friends.json               # nick, folder współdzielony, lista znajomych
├── instances/
│   └── <slug>/
│       ├── instance.json
│       ├── natives/           # rozpakowane biblioteki natywne
│       ├── logs/
│       └── .minecraft/        # odizolowany katalog gry
├── exports/                   # .zip i .mrpack z instance_export
├── meta/
│   ├── version_manifest_v2.json
│   └── versions/<id>/<id>.json
├── libraries/                 # wspólne jary, ścieżki Maven
├── assets/
│   ├── indexes/
│   ├── objects/               # adresowane SHA-1
│   └── virtual/               # układ assetów dla wersji < 1.7
├── java/
│   └── temurin-<major>/       # runtime pobrane z Adoptium
└── cache/
    ├── downloads/             # pliki .part do wznowienia
    └── packs/                 # pobrane archiwa .mrpack
```

Ścieżkę aktywnego katalogu danych zwraca komenda `data_root` — panel ustawień pokazuje ją i pozwala skopiować do schowka.

---

## Format `instance.json`

Plik leży w `<root>/instances/<id>/instance.json`. Pole `id` musi być zgodne z nazwą katalogu, w przeciwnym razie ładowanie kończy się błędem. Schemat ma wersję `1`; plik z nowszą wersją jest odrzucany.

```json
{
  "schema": 1,
  "id": "fabric-skyblock",
  "name": "Fabric Skyblock",
  "icon": null,
  "created_at": "2026-01-19T09:03:44Z",
  "updated_at": "2026-02-10T21:07:51.442Z",
  "last_played_at": "2026-02-10T21:07:51.442Z",
  "game": {
    "version": "1.20.1",
    "loader": "fabric",
    "loader_version": "0.16.5",
    "version_id": "fabric-loader-0.16.5-1.20.1"
  },
  "java": {
    "mode": "manual",
    "path": "C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.11.9-hotspot\\bin\\java.exe",
    "min_memory_mb": 1024,
    "max_memory_mb": 6144,
    "extra_args": ["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=40"]
  },
  "window": {
    "width": 1920,
    "height": 1080,
    "fullscreen": true
  },
  "jvm_args": ["-XX:+ParallelRefProcEnabled"],
  "game_args": ["--width", "1920", "--height", "1080"],
  "env": { "MESA_GL_VERSION_OVERRIDE": "4.6" },
  "notes": "Sodium, Lithium, Iris."
}
```

Pola:

| Pole | Typ | Opis |
|---|---|---|
| `schema` | `number` | Wersja schematu, obecnie `1`. |
| `id` | `string` | Slug `[a-z0-9-]`, maks. 64 znaki, zgodny z nazwą katalogu. |
| `name` | `string` | Nazwa wyświetlana. |
| `icon` | `string \| null` | Ścieżka lub identyfikator ikony instancji. |
| `created_at`, `updated_at` | RFC 3339 | Znaczniki czasu UTC. |
| `last_played_at` | RFC 3339 \| null | Ostatni start gry; sortowanie listy. |
| `game.version` | `string` | Wersja Minecrafta, np. `1.21.1`. |
| `game.loader` | `vanilla \| fabric \| quilt \| forge \| neoforge` | Loader instancji. |
| `game.loader_version` | `string \| null` | Build loadera; `null` dla Vanilli. |
| `game.version_id` | `string \| null` | Rozwiązany JSON wersji; ustawiany przez installer, wymagany do startu. |
| `java.mode` | `auto \| manual` | `auto` wybiera runtime spełniający wymagania; `manual` używa `path`. |
| `java.path` | `string \| null` | Ścieżka do `java`/`java.exe` w trybie `manual`. |
| `java.min_memory_mb`, `java.max_memory_mb` | `number` | `-Xms` / `-Xmx` w MB. |
| `java.extra_args` | `string[]` | Dodatkowe flagi JVM po flagach launchera, przed `-cp`. |
| `window.width`, `window.height` | `number` | Rozmiar okna gry (domyślnie 1280×720). |
| `window.fullscreen` | `boolean` | Start w pełnym ekranie. |
| `jvm_args` | `string[]` | Surowe flagi JVM tylko dla tej instancji. |
| `game_args` | `string[]` | Argumenty dopisywane po argumentach vanilla. |
| `env` | `object` | Zmienne środowiskowe procesu gry. |
| `notes` | `string` | Wolny tekst, nie wpływa na start. |

Gotowe przykłady: [`examples/instances/`](examples/instances/).

Wskazówki:

- `loader_version` można zostawić `null` — installer wybierze najnowszy stabilny build dla danej wersji gry.
- Po ręcznej zmianie `version` lub `loader` usuń `version_id` (ustaw `null`) i uruchom instalację ponownie; inaczej launcher wystartuje stary profil.
- `java.max_memory_mb` nie powinien przekraczać ~60% fizycznej pamięci; przy 8 GB RAM sensowne jest 4096–6144, przy 16 GB — 6144–8192.

---

## Format `settings.json`

Ustawienia globalne, leżą w katalogu głównym danych. Brakujące pola przyjmują wartości domyślne, a zakresy są przycinane przy wczytaniu (`concurrency` 1–32, `default_memory_mb` 512–65536).

```json
{
  "schema": 1,
  "theme": "dark",
  "locale": "pl-PL",
  "concurrency": 8,
  "default_memory_mb": 4096,
  "default_java": null,
  "close_on_launch": false,
  "restore_on_exit": true,
  "show_snapshots": false,
  "mojang_meta": "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
  "modrinth_api": "https://api.modrinth.com/v2",
  "curseforge_api_key": null,
  "ms_client_id": null,
  "auto_install_java": true,
  "discord_rpc": true,
  "discord_client_id": null
}
```

| Pole | Opis |
|---|---|
| `theme` | `system`, `dark` lub `light`. |
| `locale` | Tag BCP-47; pusty ciąg oznacza „jak system”. |
| `concurrency` | Liczba równoległych pobrań. |
| `default_memory_mb` | Domyślny `-Xmx` dla nowych instancji. |
| `default_java` | Preferowana ścieżka Javy dla nowych instancji. |
| `close_on_launch` | Minimalizuj launcher po starcie gry (proces działa dalej, więc `Stop` zostaje pod ręką). |
| `restore_on_exit` | Przywróć okno — z minimalizacji i na wierzch — gdy ostatnia gra się zakończy. Działa niezależnie od `close_on_launch`. |
| `show_snapshots` | Pokaż snapshoty na liście wersji. |
| `mojang_meta` | URL manifestu wersji Mojang. |
| `modrinth_api` | Bazowy URL API Modrinth. |
| `curseforge_api_key` | Pole zarezerwowane; integracja CurseForge nie jest jeszcze zaimplementowana. || `ms_client_id` | Application (client) ID z Azure. Nadpisywane zmienną `SPUD_MS_CLIENT_ID`. |
| `auto_install_java` | Pobierz JRE z Adoptium, gdy żaden zainstalowany runtime nie spełnia wymagań (tylko tryb `auto`). |
| `discord_rpc` | Wysyłaj Rich Presence do lokalnego klienta Discorda. |
| `discord_client_id` | Application ID z Discord Developer Portal. Bez niego toggle nic nie robi. |

---

## Tryb portable

Utwórz pusty plik `portable.txt` obok pliku wykonywalnego launchera. `LauncherPaths::discover` wykryje go i przeniesie cały katalog danych do `data/` obok binarki — ustawienia, instancje, biblioteki, assety i cache. Przydatne na pendrive i do testów bez zaśmiecania profilu użytkownika.

```
Spud Launcher.exe
portable.txt
data/
```

---

## Komendy Tauri

Wszystkie komendy są zarejestrowane w `src-tauri/src/lib.rs`. Argumenty skalarne wywołujesz z JS w camelCase (`instanceId`, `mcVersion`, `projectId`, `versionId`, `includeSnapshots`, `idOrSlug`, `gameVersion`, `username`), a struktury (`NewInstance`, `SearchQuery`, `DeviceCode`, `InstanceConfig`, `LauncherSettings`) mają pola w snake_case.

| Komenda | Argumenty | Wynik |
|---|---|---|
| `instances_list` | — | `InstanceSummary[]` |
| `instance_get` | `id` | `InstanceConfig` |
| `instance_create` | `draft: NewInstance` | `InstanceConfig` |
| `instance_update` | `config: InstanceConfig` | `InstanceConfig` |
| `instance_delete` | `id` | — |
| `instance_folder` | `id` | `string` |
| `instance_install` | `id` | `InstallResult` |
| `instance_launch` | `id` | `LaunchResult` |
| `instances_running` | — | `RunningGame[]` |
| `instance_stop` | `id` | `StopResult` |
| `instance_required_java` | `mcVersion` | `number` |
| `window_state` | — | `WindowState` (`fullscreen`, `maximized`, `minimized`) |
| `window_fullscreen` | `fullscreen: boolean` | `boolean` |
| `window_minimize` | `minimize: boolean` | `boolean` |
| `settings_get` / `settings_save` | — / `settings` | `LauncherSettings` |
| `java_detect` | — | `JavaRuntime[]` |
| `java_inspect` | `path` | `JavaRuntime \| null` |
| `minecraft_versions` | `includeSnapshots` | `VersionSummary[]` |
| `loader_versions` | `loader`, `mcVersion` | `LoaderBuild[]` |
| `data_root` | — | `string` |
| `accounts_list` | — | `AccountsView` |
| `account_begin_login` | — | `DeviceCode` |
| `account_finish_login` | `device: DeviceCode` | `AccountView` |
| `auth_status` | — | `AuthStatus` |
| `account_add_offline` | `username` | `AccountsView` |
| `account_remove` / `account_set_active` | `id` | `AccountsView` |
| `modrinth_search` | `query: SearchQuery` | `ModrinthSearch` |
| `modrinth_project` | `idOrSlug` | `ModrinthProject` |
| `modrinth_versions` | `projectId`, `gameVersion?`, `loader?` | `ModrinthVersion[]` |
| `mods_installed` | `instanceId` | `InstalledMod[]` |
| `mods_install` | `instanceId`, `projectId`, `versionId?` | `InstalledMod[]` |
| `mods_remove` | `instanceId`, `projectId` | `InstalledMod[]` |
| `mods_check_updates` | `instanceId` | `ModUpdate[]` |
| `optimization_install` | `instanceId` | `OptimizationResult` |
| `modpack_install` | `versionId` | `ModpackInstallResult` |
| `java_install` | `major` | `JavaInstallResult` (`major`, `version`, `path`, `bytes`) |
| `instance_logs` | `id` | `LogSnapshot` (`instance_id`, `path`, `exists`, `lines`) |
| `instance_log_stream` | `id` | `boolean` — czy tailer już działa |
| `instance_log_files` | `id` | `LogFileInfo[]` (`name`, `path`, `bytes`, `modified`) |
| `instance_log_read` | `id`, `name` | `string` — tylko z `logs/` i `crash-reports/` |
| `instance_export` | `id`, `format: "zip" \| "mrpack"` | `ExportResult` (`path`, `format`, `files`, `bytes`) |
| `instance_import` | `path` | `ImportResult` (`instance`, `files`) |
| `launchers_scan` | — | `ScanResult` (`launchers`, `instances`) |
| `launcher_import` | `path`, `name?`, `mcVersion?`, `loader?`, `loaderVersion?` | `ImportOutcome` (`instance`, `copied`, `skipped`) |
| `friends_get` / `friends_save` | — / `config: FriendsConfig` | `FriendsConfig` |
| `friends_add` / `friends_remove` | `nick` | `FriendsConfig` |
| `friends_list` | — | `FriendView[]` |
| `friends_archives` | — | `SharedArchive[]` |
| `friends_publish` | — | `PresenceFile` |
| `friends_share_instance` | `id` | `SharedArchive` |
| `friends_import` | `path` | `ImportResult` |

Zdarzenia postępu: `install://progress` i `mods://progress`, payload `DownloadEvent`:

```json
{ "type": "progress", "completed": 12, "total": 84, "bytes": 73400320, "current": "1.21.1.jar" }
{ "type": "finished", "files": 84, "bytes": 412345678 }
```

Zdarzenia okna, gry i konsoli: `window://fullscreen` (payload `boolean` — launcher wszedł lub wyszedł z pełnego ekranu, także wtedy gdy backend zdejmuje go przy starcie gry), `game://changed` (bez payloadu — lista działających instancji wymaga odświeżenia; emitowane przy starcie, przy `Stop` i przez watcher, który zauważa samodzielne wyjście z gry w ~400 ms) oraz `game://log` (payload `{ instance_id, lines }` — nowe linie z `logs/game.log`).

---

## Znane ograniczenia

- **Zweryfikowana kompilacja i start gry.** `cargo check`, `cargo build` i `cargo test` przechodzą czysto (37 testów). End-to-end sprawdzono na żywej sieci: instalacja Fabric, konto offline, zestaw optymalizacyjny i start Minecrafta 26.3 (Fabric 0.19.5, 64 mody). Import z Modrinth App sprawdzony na profilu „Fabric 26.3” (32 mody) — skan odczytał wersję z `logs/latest.log` i skopiował zawartość do nowej instancji. Eksport i import instancji sprawdzone w obie strony dla `.zip` i `.mrpack`, w tym pobranie jara z linku CDN z weryfikacją SHA-1. Nieprzetestowane: instalacja Forge/NeoForge przez oficjalny installer (`--installClient`), odpowiedź serwera Microsoft na realnym `client_id` (samo okablowanie device code jest sprawdzone testami), pobranie Javy 8 z Adoptium i Rich Presence na prawdziwym `application id`.
- Brak weryfikacji integralności bibliotek loadera przy każdym starcie — `ensure_version_files` dociąga tylko pliki nieobecne; podmieniony, ale istniejący plik nie jest hashowany (biblioteki Fabric nie mają sum w profilu).
- Instalacja Forge/NeoForge zależy od zachowania oficjalnego installera (`--installClient`) i układu katalogów staging; wymaga testu na realnej wersji gry.
- `CurseForge` — pole `curseforge_api_key` istnieje, ale klient API nie jest napisany. Import z CurseForge działa (czyta `minecraftinstance.json`), ale instalacja paczek z tego źródła nie.
- Autopobieranie Javy pokrywa tylko Adoptium Temurin. Gdy Adoptium nie ma buildu dla danej platformy (np. egzotyczny ARM), `java_install` zwraca błąd i zostaje ręczne wskazanie runtime’u.
- Konto offline nie daje skinów ani dostępu do Realms i serwerów online — to ograniczenie trybu offline, nie launchera.
- Stare wersje (alpha/beta, 1.0–1.12) wymagają Javy 8. Launcher pobierze ją, jeśli `auto_install_java` jest włączone, ale bez tego wybierze najstarszą nowszą Javę (tu: 21) — instalacja i plan startu będą poprawne, a sama gra padnie na nowoczesnym JVM. To ograniczenie gry, nie wyboru runtime.
- Zestaw optymalizacyjny zależy od tego, czy dany mod ma build na wybraną wersję gry; przy świeżym snapshocie część pozycji trafi do `skipped`.
- Interfejs jest po angielsku; struktura pod i18n (tag `locale`) jest przygotowana, ale brak plików tłumaczeń.
- Brak podpisywania kodu i auto-update.
- `close_on_launch` minimalizuje okno i zostawia launcher w pasku zadań; nie ma tray’a ani automatycznego przywracania okna po zamknięciu gry.
- Rejestr uruchomionych gier żyje w pamięci procesu launchera. Gra uruchomiona przed zamknięciem launchera działa dalej, ale po restarcie nie jest już śledzona: nie ma dla niej przycisku **Stop** i nie blokuje ponownego startu tej samej instancji.
- Zatrzymanie gry jest dwufazowe (15 s na zamknięcie okna, potem `taskkill /F`), więc przy zawieszonym procesie **Stop** odpowiada dopiero po tym czasie; UI czeka na wynik komendy.
- **Znajomi nie mają serwera.** Presence to plik w folderze, który musi widzieć każda strona — bez OneDrive/Syncthing/udziału sieciowego lista będzie pusta. „Online” to „plik zapisany w ciągu 5 minut”, a nie prawdziwe połączenie, więc zamknięcie launchera bez czekania pokaże znajomego jako online do 5 minut. Adres serwera pochodzi z linii `Connecting to`, więc dotyczy tylko serwerów, na które gracz wszedł z poziomu gry, nie tych dołączonych przez `--server`.
- **Rich Presence wymaga własnego `application id`.** Bez wpisu w Settings → *Discord* toggle jest bezczynny, a launcher nie łączy się nawet z lokalnym potokiem.
- Import z innych launcherów kopiuje pliki, ale nie przenosi kont ani ustawień launchera źródłowego. Profil bez żadnej wskazówki o wersji (np. ręcznie zrobiony katalog) trzeba uzupełnić ręcznie — instancja bez `mc_version` nie ma czego instalować.
- `.mrpack` z instancji bez manifestu `.spud/mods.json` (np. zaimportowanej z innego launchera) jest tak duży jak `.zip` — nie ma czego linkować do CDN, więc wszystkie jary jadą w `overrides/`. Format pozostaje poprawny, ale oszczędności nie ma.
- Import `.mrpack` wymaga sieci, jeśli indeks ma wpisy `files`; bez połączenia zostaną tylko pliki z `overrides/`. Import `.zip` jest w pełni offline.

---

## Licencja

MIT — patrz [`LICENSE`](LICENSE).
