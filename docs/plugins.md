# Wtyczki Spud Launcher

Wtyczka to folder w `<dane>/plugins/<id>/` z dwoma plikami:

```text
plugins/
├── state.json          # które wtyczki są włączone (tworzy launcher)
└── hello-spud/
    ├── plugin.json     # manifest
    └── main.js         # kod wtyczki
```

Folder danych znajdziesz w **Settings → Plugins → Open plugins folder** albo w
`data_root()` (na Windows: `%APPDATA%\dev\spud\SpudLauncher\data`).

## Instalacja

1. Skopiuj folder wtyczki do `plugins/`.
2. **Settings → Plugins** → zaznacz checkbox przy nazwie.
3. Panel wtyczki pojawi się w zakładce **Plugins** na pasku bocznym.

Wtyczki są **domyślnie wyłączone**. Samo wrzucenie folderu nic nie uruchamia.

## Manifest — `plugin.json`

```json
{
  "id": "hello-spud",
  "name": "Hello Spud",
  "version": "1.0.0",
  "api": 1,
  "description": "Co robi ta wtyczka",
  "author": "twoje imię",
  "main": "main.js",
  "hooks": ["game:started", "game:stopped"]
}
```

| Pole | Wymagane | Znaczenie |
|---|---|---|
| `id` | tak | `a-z`, `0-9`, `-`, `_`, maks. 64 znaki. **Musi być identyczne z nazwą folderu** — to także namespace pamięci wtyczki. |
| `name` | tak | Nazwa widoczna w UI. |
| `version` | tak | Wersja wtyczki (dowolny string, np. `1.2.0`). |
| `api` | nie (domyślnie `1`) | Wersja API. Launcher mówi API 1; inna wartość = wtyczka jest listowana, ale się nie ładuje. |
| `main` | nie (domyślnie `main.js`) | Plik wejściowy. Musi leżeć bezpośrednio w folderze wtyczki — ścieżki typu `../` są odrzucane. |
| `hooks` | nie | Deklaracja eventów, których wtyczka słucha. Informacyjna — pokazywana w UI przed włączeniem. |

Błędy manifestu nie wywalają launchera: wtyczka pojawia się na liście z opisem
problemu i wyłączonym checkboxem.

## API — obiekt `Spud`

Kod wtyczki dostaje dokładnie jeden argument. Nie ma `require`, `import`, Node
API ani dostępu do wewnętrznych poleceń Tauri.

```js
Spud.id        // "hello-spud"
Spud.name      // "Hello Spud"
Spud.version   // "1.0.0"
Spud.api       // 1

Spud.log(...args)          // console.info z prefiksem [plugin:<id>]
Spud.storage.get(key, fallback)
Spud.storage.set(key, value)   // JSON, per wtyczka
Spud.storage.remove(key)

Spud.ui.panel({ id, title, mount(element) })
Spud.ui.removePanel(id)
Spud.ui.notify(text)

Spud.events.on(event, handler) // zwraca funkcję odpinającą
```

### `Spud.ui.panel`

```js
Spud.ui.panel({
  id: "status",              // unikalne w obrębie wtyczki
  title: "Mój panel",
  mount(element) {
    element.textContent = "hello";
    return () => { /* opcjonalny cleanup */ };  // wołane przy odmontowaniu
  },
});
```

`element` to pusty `div` w karcie na stronie **Plugins**. React nigdy nie
dotyka jego wnętrza, więc wtyczka może budować DOM jak chce. Limit: 8 paneli na
wtyczkę.

### `Spud.events.on`

```js
Spud.events.on("game:started", (payload) => {
  Spud.log(payload.instanceId);
});
```

Eventy emitowane przez launcher:

| Event | Payload | Kiedy |
|---|---|---|
| `launcher:ready` | — | Po załadowaniu/ponownym załadowaniu wtyczek. |
| `view:changed` | `{ view }` | Zmiana zakładki (`instances`, `discover`, `plugins`, …). |
| `instances:changed` | `{ count }` | Lista instancji została odświeżona. |
| `game:started` | `{ instanceId, game }` | Gra wystartowała. |
| `game:stopped` | `{ instanceId }` | Gra zakończyła się lub padła. |
| `theme:changed` | `{ theme, resolved }` | Motyw systemowy/ciemny/jasny się zmienił. |

## Izolacja błędów

- Wyjątek w kodzie wejściowym → wtyczka oznaczona jako `failed`, źródło błędu
  widoczne w **Settings → Plugins** i na stronie **Plugins**.
- Wyjątek w handlerze eventu lub w `ui.panel` → liczony do budżetu wtyczki.
  Po **5 błędach** wtyczka jest zatrzymywana: handlery przestają być wołane,
  panele znikają. Reszta launchera działa dalej.
- Wyjątek w `mount()` → tylko ten panel pokazuje błąd, inne panele i wtyczki
  działają normalnie.

## Ograniczenia i bezpieczeństwo

- **Włączona wtyczka to zaufany kod.** Dzieli webview z UI launchera, więc może
  czytać DOM, `localStorage` i wykonywać `fetch`. Nie ma piaskownicy procesowej.
  Dlatego: domyślnie wyłączone, źródło trafia do webview tylko po włączeniu,
  limit rozmiaru 512 KiB, ścieżka `main` nie może wyjść z folderu.
- Dlaczego nie natywne biblioteki (`.dll`/`.so`)? Brak stabilnego ABI między
  wersjami kompilatora — każda aktualizacja launchera łamałaby każdą wtyczkę —
  a panika w natywnym kodzie zabija cały launcher. Wtyczka JS jest przenośna
  między Windows/macOS/Linux i nie może wywrócić procesu.
- Brak telemetrii: launcher nie raportuje, jakie wtyczki są zainstalowane.
- Sekrety: nie wkładaj tokenów do `plugin.json` ani do `main.js` — oba pliki są
  zwykłym tekstem w folderze danych.

## Przykład

`examples/plugins/hello-spud/` w repozytorium: licznik uruchomień, reakcja na
`game:started` / `game:stopped` / `instances:changed`, panel z przyciskiem i
pamięcią w `Spud.storage`. Skopiuj folder, włącz, sprawdź.
