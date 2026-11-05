# Overlay w grze

Drugie okno launchera: zawsze na wierzchu, przezroczyste, bez ramki. Pokazuje liczniki maszyny, uruchomioną instancję i listę jej modów. Wywoływane globalnym skrótem, domyślnie **Shift+Tab**.

To jest **osobne okno Tauri**, nie nakładka rysowana wewnątrz klienta Minecrafta. Launcher nie wstrzykuje nic do JVM, nie dotyka procesu gry i nie czyta jej pamięci — dlatego działa z każdą wersją gry, każdym loaderem i nie może wysypać gry.

---

## Włączenie

**Settings → Overlay**:

1. Zaznacz **Enable the in-game overlay**.
2. Wpisz skrót (format `Modifier+Key`, np. `Shift+Tab`, `Ctrl+Shift+O`, `Alt+F1`).
3. Zapisz formularz przyciskiem **Save**.

Skrót jest rejestrowany od razu po zapisie i wyrejestrowywany po odznaczeniu — bez restartu launchera. **Preview overlay** pokazuje okno także przy wyłączonym przełączniku, żebyś zobaczył, co włączasz.

Domyślnie overlay jest **wyłączony**. Nikt nie dostaje okna na wierzchu, o które nie poprosił.

---

## Co pokazuje

| Sekcja | Zawartość | Źródło |
|---|---|---|
| Nagłówek | znacznik `SPUD.`, aktualny skrót, przycisk zamykania | ustawienia |
| Instancja | nazwa, wersja gry, loader i jego wersja, czas od startu | `RunningGames` + `instance.json` |
| CPU | udział zajętego czasu procesora od poprzedniego odczytu | `GetSystemTimes` / `/proc/stat` |
| System RAM | zajęte / całość | `GlobalMemoryStatusEx` / `/proc/meminfo` |
| Game RSS | pamięć rezydentna procesu gry | `GetProcessMemoryInfo` / `/proc/<pid>/statm` |
| Heap cap | `-Xmx` instancji | `instance.json` |
| PID | identyfikator procesu gry | rejestr procesów |
| Mody | liczba i lista (7 pozycji + „+N more”) | `.spud/mods.json` |

Odświeżanie: raz na sekundę, tylko gdy okno jest widoczne (`overlay://shown` wymusza natychmiastowy odczyt po pokazaniu). Gdy nic nie działa, panel pokazuje liczniki maszyny i podpis „No game running” — overlay nie wymaga uruchomionej gry.

---

## Skrót klawiszowy

- Rejestrowany przez `tauri-plugin-global-shortcut`, czyli **globalnie w systemie** — to jedyny sposób, żeby zadziałał, gdy fokus ma Minecraft.
- Konsekwencja nazwana wprost: dopóki overlay jest włączony, gra **nie dostaje** tego skrótu. `Shift+Tab` przestaje cokolwiek robić w innych programach.
- Nieprawidłowy format (`Shift+`, `Ctrl+`, `F`, puste pole) nie blokuje zapisu ustawień — panel pokazuje powód odmowy, a skrót zostaje wyrejestrowany. Puste pole wraca do `Shift+Tab` przy zapisie.
- Zmiana skrótu wymaga zapisu formularza: pole edytuje wersję roboczą, komendy czytają zapisane ustawienia.

---

## Ograniczenia (przeczytaj przed włączeniem)

- **Pełny ekran wykluczający (exclusive fullscreen) zasłania wszystko.** Minecraft w trybie exclusive oddaje obraz bezpośrednio do sterownika i żadne okno systemowe nie narysuje się nad nim. Ustaw **Windowed** albo **Borderless** w opcjach gry. To ograniczenie systemu, nie launchera.
- **Brak licznika FPS.** Prawdziwe FPS zna tylko pętla renderowania gry. Odczyt wymagałby wstrzyknięcia kodu do JVM (agent, ASM, mieszanie w `Minecraft.runTick`), co jest kruche przy każdej aktualizacji gry i może ją wysypać. Overlay pokazuje CPU, RAM i RSS — czyli to, co da się zmierzyć z zewnątrz, uczciwie i bez ryzyka.
- **Windows: okno nigdy nie dostaje fokusu.** Ustawiane są style `WS_EX_NOACTIVATE` i `WS_EX_TOOLWINDOW`, żeby pokazanie overlaya nie zatrzymywało gry (Alt-Tab do okna w tle potrafi zminimalizować grę). Skutek: overlay nie przyjmuje klawiatury — zamyka się tym samym skrótem albo przyciskiem ✕. Kliknięcia myszą działają.
- **macOS: pokazanie okna może zabrać fokus.** Okno jest tworzone z `focus: false` i launcher nigdy nie woła `set_focus`, ale system może zdecydować inaczej. Dlatego domyślnym skrótem jest `Shift+Tab`, a nie klawisz funkcyjny, który gra wykorzystuje.
- **Linux: przezroczystość wymaga kompozytora.** Bez niego (np. czysty X11 z wyłączonym compositingiem) tło będzie czarne zamiast przezroczystego — panel nadal działa.
- **Brak historii.** Overlay pokazuje bieżący odczyt, nie wykres. Zero buforowania, zero plików, zero zbierania danych w tle.

---

## Architektura

| Element | Plik |
|---|---|
| Okno (`label: "overlay"`) | `src-tauri/tauri.conf.json` |
| Uprawnienia okna | `src-tauri/capabilities/overlay.json` (`core:default` i nic więcej) |
| Komendy, skrót, style Windows | `src-tauri/src/commands/overlay.rs` |
| Sampler CPU/RAM/RSS | `crates/launcher-core/src/metrics.rs` |
| Entry point UI | `ui/overlay.html` + `ui/src/overlay.tsx` |
| Komponent | `ui/src/components/OverlayApp.tsx` |
| Style (osobny plik, bez `app.css`) | `ui/src/styles/overlay.css` |

Decyzje, które warto znać:

- **Osobne okno, nie widok w launcherze.** Okno launchera zminimalizowane pod grą nie da się pokazać bez zabrania fokusu. Osobne okno z `alwaysOnTop` i `skipTaskbar` to jedyny wariant, który nie przeszkadza w grze.
- **Osobny entry point Vite.** `vite.config.ts` ma dwa wejścia (`index.html`, `overlay.html`), więc overlay nie ciągnie całego shella aplikacji: ~5 kB JS i ~4,5 kB CSS zamiast ~150 kB.
- **Pomiar CPU jest różnicą, nie migawką.** Pierwszy odczyt nie ma odpowiedzi i zwraca `None` (UI rysuje „—”), bo wymyślone 0% byłoby kłamstwem. Liczniki systemowe są monotoniczne, więc odejmowanie jest bezpieczne; licznik, który się cofnął, też daje `None`.
- **Brak workera w tle.** Sampler żyje w `OnceLock` i jest wołany tylko wtedy, gdy overlay pyta o payload. Zamknięty overlay nie kosztuje nic.
- **macOS bez CPU.** Odczyt ticków wymaga Mach host statistics, czyli innej tabeli bindingów niż `sysctlbyname` używane przez `hardware.rs`. Zły licznik w overlayu jest gorszy niż pusta pozycja, więc macOS pokazuje RAM, a CPU zostaje „—”.

---

## Format `settings.json`

```json
{
  "overlay_enabled": false,
  "overlay_hotkey": "Shift+Tab"
}
```

| Pole | Opis |
|---|---|
| `overlay_enabled` | Czy rejestrować skrót i pozwolić oknu działać. Domyślnie `false`. |
| `overlay_hotkey` | Kombinacja w formacie `Modifier+Key`. Puste pole wraca do `Shift+Tab`; wartość jest przycinana do 64 znaków. |

Oba pola mają `#[serde(default)]` — stary `settings.json` ładuje się bez migracji.

---

## Komendy

| Komenda | Argumenty | Wynik |
|---|---|---|
| `overlay_status` | — | `OverlayStatus` (`enabled`, `hotkey`, `registered`, `error`, `window`, `visible`) |
| `overlay_payload` | — | `OverlayPayload` (`instance`, `metrics`, `mods`, `mods_total`, `captured_at`) |
| `overlay_toggle` | `show?: boolean` | `boolean` — nowy stan widoczności |
| `overlay_close` | — | `boolean` — zawsze `false` |

`overlay_toggle` z jawnym `show: true` działa także przy wyłączonym przełączniku — to podgląd z ustawień.

---

## Rozwiązywanie problemów

| Objaw | Przyczyna | Co zrobić |
|---|---|---|
| Skrót nic nie robi | overlay wyłączony albo skrót przechwycony przez inny program | sprawdź **Settings → Overlay**, czy status mówi `Registered` |
| `The system refused …` | kombinacja zajęta przez system lub inne okno | wpisz inną, np. `Ctrl+Shift+O` |
| Okno jest, ale gra go zasłania | pełny ekran wykluczający | przełącz grę na Borderless |
| Czarny prostokąt zamiast panelu (Linux) | brak kompozytora | włącz compositing |
| „Counters unavailable” | proces gry zakończył się między odczytami albo system odmówił dostępu | zamknij i pokaż overlay ponownie |
| Brak klawiatury w overlayu (Windows) | `WS_EX_NOACTIVATE` — celowe | użyj skrótu albo ✕ |
