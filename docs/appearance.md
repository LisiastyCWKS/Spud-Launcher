# Personalizacja wyglądu

Paleta, gęstość, zaokrąglenia, szerokości kolumn i własne tło — wszystko pod **Settings → Appearance → Personalization**. Ustawienia leżą w `settings.json`, więc przeżywają restart i działają w trybie portable tak samo jak w instalacji.

Zasada, na której to stoi: **żaden komponent nie zna motywu**. Personalizacja to wyłącznie zmienne CSS i trzy atrybuty na `<html>` (`data-preset`, `data-density`, `data-corners`). Dzięki temu nowy widok nie potrzebuje ani jednej linii o motywach.

---

## Palety

| Preset | Charakter | `data-preset` |
|---|---|---|
| **Spud** | Bazowa zieleń, dokładnie taka jak przed tą wersją | `spud` |
| **Midnight** | Granatowa czerń, chłodne akcenty | `midnight` |
| **Forest** | Głęboka zieleń i kora | `forest` |
| **Dusk** | Śliwka i żar | `dusk` |
| **Paper** | Ciepła biel pod pracę w dzień | `paper` |

Preset nadpisuje **tylko kolory** (`--bg`, `--surface*`, `--border*`, `--text*`, `--muted`, `--danger`, `--warning`). Nie rusza `--radius`, czasu animacji ani układu — te mają osobne przełączniki. Presety mają wariant ciemny i jasny, więc zmiana motywu w sidebarze nie gubi wybranej palety.

`Spud` nie nadpisuje niczego: to paleta z `theme.css`, zachowana 1:1, żeby aktualizacja nie zmieniła wyglądu nikomu, kto nic nie wybrał.

---

## Układ

| Ustawienie | Zakres | Domyślnie | Efekt |
|---|---|---|---|
| `density` | `comfortable`, `compact` | `comfortable` | Wysokość wierszy, padding kart, wysokość topbara, odstępy siatki |
| `corners` | `sharp`, `soft`, `round` | `soft` | `--radius` 3 / 10 / 16 px i `--radius-sm` 2 / 7 / 11 px |
| `sidebar_width` | 200–320 px | 236 | Szerokość kolumny nawigacji (`--sidebar`) |
| `content_width` | 960–1800 px | 1240 | Maksymalna szerokość treści (`--content-max`) |
| `reduce_motion` | `true` / `false` | `false` | Wyłącza **wszystkie** przejścia i animacje, niezależnie od ustawień systemu |

`compact` nie ukrywa żadnego elementu i nie schodzi poniżej 28 px celu kliknięcia. Poniżej 1024 px szerokości okna sidebar i tak zwija się do ikon — to zachowanie sprzed personalizacji, niezmienione.

`reduce_motion` to twardy wyłącznik: `:root[data-motion="off"] * { transition: none !important; animation: none !important; }`. Działa także wtedy, gdy system pozwala na ruch, i odwrotnie — systemowe `prefers-reduced-motion` nadal wygrywa.

---

## Własne tło

1. **Settings → Appearance → Open backgrounds folder**.
2. Wrzuć obraz do `<data>/backgrounds/` — PNG, JPEG, WebP, GIF, AVIF albo BMP.
3. **Rescan folder**, potem kliknij kafelek.

| Ustawienie | Zakres | Domyślnie |
|---|---|---|
| `background` | nazwa pliku albo `""` | `""` (brak) |
| `background_opacity` | 0–100 | 32 |
| `background_blur` | 0–40 px | 14 |

Obraz jest rysowany na `body::before` pod całym shellem i przyciemniany kolorem motywu (gradient `--bg` 34 % → 58 %), dlatego ten sam plik czyta się dobrze i w ciemnym, i w jasnym motywie. Gdy tło jest aktywne, sidebar i topbar robią się półprzezroczyste z `backdrop-filter: blur(20px)` — inaczej nieprzezroczyste panele zasłoniłyby jedyną rzecz, którą właśnie ustawiono.

### Bezpieczeństwo ścieżek

Launcher **nie serwuje dowolnych ścieżek**. Tło jest czytane komendą `background_data`, która:

- przyjmuje wyłącznie **samą nazwę pliku** — separatory, `..`, dwukropek, znaki kontrolne i wiodąca kropka są odrzucane,
- wymaga rozszerzenia z whitelisty (SVG celowo nie ma — to dokument, który potrafi nieść skrypt),
- czyta tylko z `<data>/backgrounds/`,
- odmawia plików większych niż 24 MiB i pustych,
- zwraca `data:` URL zamiast udostępniać protokół assetów.

Nieudany odczyt (plik usunięty, nazwa z ręcznie edytowanego `settings.json`) to „brak tła”, nie błąd startu — `sanitise()` czyści niepoprawną nazwę przy wczytaniu ustawień.

### Ikony i podgląd

Panel dekoduje **tylko wybrane** tło (jeden podgląd 21:7). Lista to kafle z nazwami i rozmiarami bez wczytywania obrazów — folder z dwudziestoma tapetami 4K nie zjada pamięci na miniatury.

---

## Plik ustawień

```json
{
  "theme": "dark",
  "theme_preset": "midnight",
  "density": "compact",
  "corners": "round",
  "background": "mountains.jpg",
  "background_opacity": 42,
  "background_blur": 18,
  "sidebar_width": 248,
  "content_width": 1440,
  "reduce_motion": false
}
```

Wszystkie pola mają `#[serde(default)]`: stary `settings.json` bez tych kluczy wczytuje się bez migracji i dostaje wartości domyślne. Zakresy są przycinane w `LauncherSettings::sanitise()` — ręcznie wpisane `sidebar_width: 5000` wczyta się jako 320, a `background: "../etc/passwd"` jako brak tła.

---

## Podgląd bez zapisu

Suwaki i kafle działają natychmiast: panel podnosi szkic do `App`, a `useAppearance` aplikuje go na `<html>`. Zapis do `settings.json` następuje dopiero po **Save settings**. Wyjście z zakładki Settings bez zapisu przywraca wygląd z pliku — podgląd nigdy nie wycieka do reszty launchera.

---

## Znane ograniczenia

- **Tło jest stałe, nie przewijane.** `background-attachment: fixed` na pseudo-elemencie `body` — obraz nie jedzie z treścią. To celowe: przewijające się tło kosztuje repaint przy każdej klatce.
- **Brak animowanych tapet.** GIF jest odtwarzany przez WebView, ale nie ma sterowania (pauza, tempo) i nie jest to wspierane jako funkcja.
- **Brak przycinania/miniaturyzacji.** Plik jest wysyłany do webview w oryginale; 24 MiB to sufit. Ogromny PNG (np. 8000×8000) będzie wolno się dekodował przy pierwszym pokazaniu.
- **Motyw jasny + ciemne tło bywa kontrastowe.** Tint liczy się z `--bg`, więc jasny motyw rozjaśnia obraz. Przy `background_opacity` 100 i bardzo ciemnej tapiecie gradient może nie wystarczyć — zejdź z kryciem albo zwiększ blur.
- **`backdrop-filter` na Linuksie** wymaga kompozytora; bez niego panele będą jednolicie przyciemnione zamiast rozmywać tło.

---

## Pliki

| Plik | Rola |
|---|---|
| `crates/launcher-core/src/appearance.rs` | Lista teł, walidacja nazw, MIME, `data:` URL + 7 testów |
| `crates/launcher-core/src/settings.rs` | Pola, zakresy, `sanitise()` + testy |
| `src-tauri/src/commands/appearance.rs` | `backgrounds_list`, `background_data`, `backgrounds_open_folder` |
| `ui/src/hooks/useAppearance.ts` | Aplikacja zmiennych CSS i atrybutów, cache obrazów |
| `ui/src/components/AppearancePanel.tsx` | Panel w Settings |
| `ui/src/styles/theme.css` | Presety, zaokrąglenia, wyłącznik ruchu |
| `ui/src/styles/app.css` | Warstwa tła, glass panele, gęstość, style edytora |

---

## Warstwa 3.0 (`v3.css`)

Od 3.0 wygląd siedzi w trzech plikach, ładowanych w tej kolejności: `theme.css` (tokeny), `app.css` (komponenty) i `v3.css` (powłoka). Nowy plik nie rośnie w miejscu `app.css`, bo ten ma już 2615 linii — dopiski wchodzą do warstwy, która wygrywa kaskadą, nie `!important`.

| Token | Rola |
|---|---|
| `--elev-1…4` | Skala elevacji: karta, karta w hoverze, popover, dialog |
| `--glass`, `--glass-strong`, `--glass-border` | Trzy gęstości szkła na `color-mix()` |
| `--sheen` | Gradient światła od góry — bez niego samo `backdrop-filter` czyta się jak błoto |
| `--ring`, `--ring-soft` | Pierścienie focusu i poświata pól |
| `--skeleton` | Tło szkieletów ładowania |

`body::after` rysuje dwie bardzo niskie plamy akcentu pod shellem. Dają szkłu co refraktować i odbierają ciemnemu motywowi płaskość — `z-index: -1`, więc nie dotykają żadnego kontekstu nakładania.

### Płynna zmiana motywu

`useTheme` ustawia `data-theme-switching` na `<html>` tylko na czas podmiany palety (320 ms), potem atrybut znika. Zostawiony na stałe zamieniłby każdy późniejszy hover w 340-milisekundowe przejście koloru.

### Sidebar jako rail ikon

Poniżej 880 px kolumna zwija się do 64 px: etykiety znikają, marka zostaje sama, a każdy przycisk trzyma swój `title`, więc natywny tooltip dalej mówi, co robi. To czysty CSS — żadnego stanu w JS.

### Toast stack i dialog

Style obu elementów leżą w `v3.css` (sekcje 9 i 10), razem z `@keyframes toast-in` / `toast-out` / `toast-timer`. Pasek odliczania używa `animationDuration` ustawionego z tej samej liczby, którą planuje store (`lib/toast.ts`), więc nigdy się nie rozjeżdżają. `prefers-reduced-motion` chowa pasek całkowicie — komunikat nie zależy od ruchu.

| Plik | Rola |
|---|---|
| `ui/src/styles/v3.css` | Elevacja, szkło, poświata, przyciski, inputy, toast, confirm, responsywność |
| `ui/src/lib/toast.ts` | Store powiadomień: limit 4, dedupe, pauza na hover, `duration: 0` |
| `ui/src/components/Toaster.tsx` | Renderer stosu, `useSyncExternalStore` |
| `ui/src/lib/confirm.ts` | Promise-based kolejka potwierdzeń |
| `ui/src/components/ConfirmHost.tsx` | `alertdialog`, pułapka focusu, Escape i tło anulują |
