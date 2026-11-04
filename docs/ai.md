# Analiza crashy (AI)

Dwa poziomy, celowo rozdzielone:

1. **Reguły offline** — czysty tekstowy analizator w Rust, bez sieci, bez klucza i
   bez zgody. Działa zawsze i to on pokazuje się pierwszy.
2. **Wyjaśnienie modelu** — opcjonalne, po jawnym potwierdzeniu dla **każdego
   pojedynczego żądania**. Nigdy w tle.

Model nigdy nie jest jedyną odpowiedzią. Jeśli provider jest wyłączony albo nie
ma klucza, reguły offline dalej dają pełną diagnozę.

## Co robią reguły offline

Analizator czyta ostatnie 256 KiB pliku (reszta logu to zwykle stare, nieistotne
wpisy) i dopasowuje wzorce. Każde trafienie ma: id reguły, wagę, dowód (dokładną
linię z logu) i numerowaną listę kroków naprawczych po polsku.

| Reguła | Co wykrywa |
|---|---|
| `out-of-memory` | `Java heap space`, `OutOfMemoryError`, `GC overhead limit exceeded` |
| `java-too-old` | `UnsupportedClassVersionError` + `class file version` → konkretna wersja Javy |
| `missing-dependency` | `requires … which is missing`, brakujące `fabric-api` / `architectury` |
| `duplicate-mod` | `Duplicate mods`, `already registered` |
| `mixin-conflict` | `Mixin apply failed`, `InvalidMixinException` |
| `class-missing` | `NoClassDefFoundError`, `ClassNotFoundException` |
| `loader-mismatch` | mod na inny loader / inną wersję gry |
| `graphics` | `GLFW error`, `OpenGL`, `Pixel format not accelerated` |
| `native-crash` | `EXCEPTION_ACCESS_VIOLATION`, `0xc0000005` |
| `optifine` | OptiFine w zestawie (częste konflikty) |
| `corrupted-jar` | `ZipException`, `invalid LOC header` |
| `auth` | `Invalid credentials`, `AuthenticationException`, `Invalid session` |
| `unknown` | log wygląda na crash, ale nie pasuje do wzorców |

Analiza zwraca też `summary` (nagłówek crashu), `exception` (klasa i treść
wyjątku) oraz listę modów wspomnianych w logu (`mods/*.jar`).

## Włączenie

**Settings → AI crash analysis**:

1. Zaznacz *Explain crashes with an AI provider*.
2. Wybierz providera: `openai` (OpenAI, Ollama, LM Studio, OpenRouter, Groq —
   wszystko, co mówi `/chat/completions`) albo `anthropic` (`/v1/messages`).
3. Podaj **Endpoint** (sama baza URL, bez `/chat/completions`) i **Model**.
4. Wklej klucz API i naciśnij **Save**.
5. Zapisz ustawienia przyciskiem w stopce (przełącznik i provider są zwykłymi
   ustawieniami w `settings.json`).

Domyślne wartości: `https://api.openai.com/v1`, `gpt-4o-mini`, limit promptu
64 KB (zakres 4–512 KB).

Lokalny model bez klucza — Ollama:

```text
Endpoint: http://localhost:11434/v1
Model:    llama3.1:8b
API key:  dowolny niepusty string (Ollama go nie sprawdza)
```

## Gdzie żyje klucz

Klucz API **nie jest zapisywany w `settings.json`**. Trafia do magazynu
poświadczeń systemu (Windows Credential Manager, macOS Keychain, Secret
Service) pod usługą `spud-launcher` i wpisem `ai-api-key` — tym samym, którego
używają tokeny Microsoft. Launcher nigdy nie odczytuje klucza do UI: panel
pokazuje tylko „jest / nie ma".

Na systemie bez magazynu poświadczeń (np. headless Linux bez Secret Service)
panel pokazuje, że klucza nie da się zapisać, i analiza AI zostaje wyłączona.
Reguły offline działają dalej.

Klucz nigdy nie trafia do logów ani do komunikatu błędu. Odpowiedź błędu
providera jest obcinana do 300 znaków i również nie zawiera klucza.

## Zgoda i co dokładnie wychodzi z maszyny

Zgoda jest **per żądanie**, nie „na zawsze”. UI robi dwa wywołania:

1. `ai_analyze_crash(instanceId, name, consent: false)` — tylko lokalnie. Zwraca
   diagnozy, podsumowanie **oraz podgląd ocenzurowanego tekstu** i licznik tego,
   co usunięto.
2. Po naciśnięciu *Send redacted log and explain* —
   `ai_analyze_crash(…, consent: true)`. Dopiero to żądanie idzie do providera.

Zamknięcie okna anuluje zgodę; nie ma zapamiętywania decyzji.

Przed wysłaniem tekst przechodzi przez redaktor:

| Co usuwane | Przykład → wynik |
|---|---|
| Katalogi domowe | `C:\Users\olivia\…` → `C:\Users\<user>\…`, `/home/olivia/…` → `/home/<user>/…` |
| Flagi z sekretami | `--accessToken eyJ…` → `--accessToken <redacted>` (także `--session`, `--clientId`, `--xuid`) |
| Tokeny w JSON / nagłówkach | `"accessToken":"…"`, `"refresh_token": "…"`, `Bearer …` → `<redacted>` |
| UUID graczy i sesji | `069a79f4-44e9-4729-…` → `<uuid>` |
| Adresy e-mail | `steve@example.com` → `<email>` |

Redakcja jest nieodwracalna — placeholder nie jest tokenem, którego da się
zamienić z powrotem.

Do providera idą: **ocenzurowany log** i **diagnozy offline** (tytuł + dowód).
Nie idą: nazwa instancji, ścieżki, lista kont, ustawienia, cokolwiek innego.

## Ograniczenia

- Model może się mylić. Dostaje wstępną analizę offline, więc ma punkt zaczepienia,
  ale halucynacja nazwą moda jest możliwa — dlatego reguły offline są zawsze na
  wierzchu, a nie pod tekstem modelu.
- Analiza idzie z przyciętego logu (256 KiB ogona do reguł, limit z ustawień do
  promptu). Jeśli przyczyna jest na początku bardzo długiego logu, model jej nie
  zobaczy.
- Provider bez obsługi `/chat/completions` lub `/v1/messages` nie zadziała;
  egzotyczne API wymaga własnego adaptera w `crates/launcher-ai/src/provider.rs`.
- Limit czasu żądania to 45 s — wolny gateway kończy się komunikatem, a analiza
  offline zostaje ważna.
- Brak streamingu: odpowiedź pojawia się po zakończeniu żądania.
- Testy pokrywają heurystyki, redakcję i walidację konfiguracji (16 testów w
  `launcher-ai`). **Żadne wywołanie prawdziwego providera nie zostało wykonane**
  — do sprawdzenia z własnym kluczem.

## Kod

- `crates/launcher-ai/src/heuristics.rs` — reguły offline (`analyze`)
- `crates/launcher-ai/src/redact.rs` — redakcja przed wysłaniem
- `crates/launcher-ai/src/provider.rs` — klient OpenAI-compatible i Anthropic
- `src-tauri/src/commands/ai.rs` — komendy, keyring, zgoda per żądanie
- `ui/src/components/AiPanel.tsx` — ustawienia w Settings
- `ui/src/components/CrashAnalysisPanel.tsx` — okno analizy w Diagnostics
