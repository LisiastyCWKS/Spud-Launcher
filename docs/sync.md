# Synchronizacja w chmurze

Backup i przywracanie instancji do folderu, który kontrolujesz: dysk lokalny, dysk sieciowy, zamontowany NAS albo katalog, który i tak synchronizuje inny program (OneDrive, Google Drive, Syncthing, Dropbox).

Launcher **nie ma demona, workera ani uploadera w tle.** Push to Twoje kliknięcie, pull to Twoje kliknięcie. To jest świadoma decyzja: usługa działająca w tle na maszynie, na której ktoś gra w Minecrafta, kosztuje RAM i CPU oraz wysyła dane bez pytania.

---

## Jak to działa

```
<wybrany folder>/
└── instances/
    └── fabric-26-3/
        ├── manifest.json
        └── snapshots/
            ├── 20261104-183012-1f3c9ab2.zip
            └── 20261103-101155-77aa01de.zip
```

- Jedna instancja = jeden folder na zdalnym nośniku, jeden `manifest.json` i lista snapshotów.
- Snapshot to **cały** eksport instancji: `.minecraft`, światy, configi, resource packi, shadery i jary wgrane ręcznie. Ten sam kod, którego używa **Eksport instancji**, więc backup i eksport są bajt w bajt tym samym plikiem.
- Nazwa pliku to `RRRRMMDD-HHMMSS-<sha256[0..8]>.zip` — znacznik czasu *oraz* prefiks hasha. Dwie maszyny wysyłające w tej samej minucie nie nadpiszą się nawzajem.
- `manifest.json` jest zapisywany **na końcu**, atomowo (zapis do `.tmp` i rename). Snapshot, który się wgrał, ale nie trafił do manifestu, jest po prostu niewidoczny — czyli w bezpieczną stronę.

### Deduplikacja wysyłki

Przed wysłaniem launcher liczy SHA-256 archiwum. Jeśli takie bajty już leżą na zdalnym nośniku (w dowolnym snapshocie tej instancji), **nic nie jest wysyłane** — push kończy się komunikatem „byte-identical". Dlatego „Back up" można wcisnąć bez namysłu: nie zmieniłeś nic w instancji, nie poszło nic przez sieć.

### Retencja

Po każdym pushu manifest jest przycinany do `sync_keep` najnowszych snapshotów (1–50, domyślnie 5), a nadmiarowe archiwa są usuwane ze zdalnego nośnika. Najnowszy nigdy nie wypada: `keep` jest podłogą przycięte do 1.

---

## Polityka konfliktów

**Restore nigdy nie nadpisuje lokalnej instancji.** Pobrany snapshot przechodzi przez normalny import (ten sam, co „Import instancji"), więc powstaje **nowa** instancja obok istniejącej.

Konsekwencje, nazwane wprost:

- Dwie maszyny mogą swobodnie robić backup i restore w obie strony i żadna nie skasuje drugiej po cichu.
- „Chcę zastąpić swoją kopię tą ze zdalnego" = usuń lokalną instancję, potem przywróć. To jest jawne działanie, nie merge.
- Launcher nie robi automatycznego merge'a światów, configów ani listy modów — i nie udaje, że robi.

### Wykrywanie „nowsze na zdalnym"

Lokalny plik `<root>/sync/state.json` (nigdy nie opuszcza maszyny) pamięta ostatni `updated_at` manifestu, który widziałeś. Jeśli zdalny manifest ma nowszą datę, UI pokazuje przy instancji plakietkę **newer on remote** — czyli inna maszyna coś wysłała, odkąd ostatnio patrzyłeś.

Plik stanu jest wyłącznie wygodą: uszkodzony albo skasowany oznacza „nigdy nie synchronizowano", kosztuje znacznik czasu, nie backup.

---

## Weryfikacja integralności

Każdy snapshot ma w manifeście SHA-256 i rozmiar. Pull pobiera plik, liczy hash i porównuje:

- zgodny → plik idzie do importu,
- niezgodny → plik jest **usuwany**, a operacja kończy się błędem `snapshot … failed verification`. Uszkodzone bajty nigdy nie trafiają do instancji.

Manifest ze schematem nowszym niż rozumie launcher jest odrzucany (`UnsupportedSchema`) zamiast zgadywany — lepiej powiedzieć „zaktualizuj launcher", niż źle odczytać cudzy format.

---

## Interfejs

**Settings → Cloud sync**:

- **Backup folder** — ścieżka albo **Browse**. Przycisk **Open** otwiera folder w eksploratorze systemu (komenda nie przyjmuje argumentu ścieżki: otwiera wyłącznie to, co jest w ustawieniach).
- **Snapshots kept per instance** — retencja.
- **This device** — lista instancji z datą ostatniego backupu, ostatniego restore i przyciskiem **Back up**.
- **On the remote** — co leży na zdalnym nośniku: data, rozmiar, wersja gry, loader, urządzenie, z którego przyszło, notatka, oraz **Restore** i kosz.

Panel edytuje **wersję roboczą** formularza, a komendy czytają **zapisane** ustawienia. Jeśli ścieżka w formularzu różni się od zapisanej, panel pokazuje ostrzeżenie „save the form first" — zamiast wywalić się komunikatem, że sync nie jest skonfigurowany, mimo wpisanej ścieżki.

### Stany, które panel rozróżnia

| Stan | Znaczenie |
|---|---|
| `configured: false` | brak `sync_folder` — sync wyłączony |
| `reachable: false` | ścieżka jest, ale katalog nie istnieje / dysk odłączony |
| `writable: false` | katalog jest, ale probe (`write` + `remove` pliku `.spud-write-test`) się nie udał — backup nie zadziała, restore tak |
| wszystko `true` | push i pull dostępne |

---

## Format `settings.json`

```json
{
  "sync_backend": "folder",
  "sync_folder": "D:\\\\SpudBackups",
  "sync_keep": 5
}
```

| Pole | Opis |
|---|---|
| `sync_backend` | `folder`. Inne backendy (S3, Dropbox) wchodzą w to samo sześciometodowe API, ale nie są zaimplementowane. |
| `sync_folder` | Ścieżka do folderu backupów. Pusty ciąg = sync wyłączony (przycinany do `null` przy zapisie). |
| `sync_keep` | Liczba snapshotów na instancję, przycinana do 1–50. |

Wszystkie trzy pola mają `#[serde(default)]` — stary `settings.json` ładuje się bez migracji.

---

## Implementacja

Nowy crate `crates/launcher-sync`, **bez zależności od `launcher-core`** (core trzyma swoje helpery JSON jako `pub(crate)`), więc całość testuje się bez Tauri i bez sieci:

| Plik | Zawartość |
|---|---|
| `src/provider.rs` | trait `SyncProvider` (6 metod), `FolderProvider`, `safe_key`, `sha256_file` |
| `src/manifest.rs` | `SyncManifest`, `Snapshot`, `SnapshotMeta`, `retention` |
| `src/state.rs` | `SyncState`, `InstanceSyncState`, `remote_is_newer` |
| `src/lib.rs` | `push`, `pull`, `remotes`, `remove_snapshot`, `PushOutcome`, `SyncError` |

Warstwa komend (`src-tauri/src/commands/sync.rs`) jest cienka: archiwizacja to istniejący `instance_export_to(..., "zip", ...)`, przywracanie to istniejący `import_archive`. Dzięki temu backup zawiera dokładnie to, co eksport, a restore przechodzi przez staging katalog i walidację importu.

### `SyncProvider`

```rust
pub trait SyncProvider {
    fn id(&self) -> &'static str;
    fn describe(&self) -> String;
    fn put_file(&self, key: &str, source: &Path) -> Result<()>;
    fn get_file(&self, key: &str, target: &Path) -> Result<()>;
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn write(&self, key: &str, bytes: &[u8]) -> Result<()>;
    fn list(&self, prefix: &str) -> Result<Vec<String>>;
    fn remove(&self, key: &str) -> Result<()>;
}
```

Klucze są **walidowane, nigdy naprawiane**: `..`, ścieżka absolutna, litera dysku, NUL, pusty segment — odrzucenie z `SyncError::UnsafeKey`. Provider, który po cichu zamieniłby `../x` na `x`, zamieniłby literówkę w zapis poza folderem backupów.

### Dodanie S3 / Dropbox

Trzeba zaimplementować sześć metod na wybranym API (dla S3: `PutObject`, `GetObject`, `HeadObject`/`GetObject`, `ListObjectsV2`, `DeleteObject`; dla Dropbox: `files/upload`, `files/download`, `files/list_folder`, `files/delete_v2`) i dodać wariant do `SyncBackend` oraz konstrukcję providera w `commands::sync::provider`. Reszta — manifest, retencja, hash, konflikt — zostaje bez zmian, bo nie wie, czym jest nośnik.

Poświadczenia dla takiego backendu należą do keyringu systemu (`spud-launcher` / nazwa backendu), tak jak klucz API w warstwie AI — nigdy do `settings.json` i nigdy do logów.

---

## Ograniczenia

- **Tylko folder.** S3 i Dropbox są zaprojektowane, ale nie zaimplementowane — lepiej powiedzieć to wprost niż wysłać pół działający uploader z tokenem w pliku konfiguracyjnym.
- **Brak automatycznego merge'a.** Patrz „Polityka konfliktów".
- **Brak szyfrowania po stronie launchera.** Snapshot to zwykły `.zip`. Jeśli folder jest w chmurze zewnętrznej, szyfrowanie jest zadaniem tego folderu (albo np. VeraCrypt), a nie launchera.
- **Brak przyrostowych snapshotów.** Każdy snapshot to pełny eksport. Deduplikacja działa na poziomie „te same bajty = brak wysyłki", nie na poziomie „zmienił się jeden config".
- **Rozmiar.** Duża instancja z shaderami i światami to setki MB na snapshot; `sync_keep` liczy się wtedy w gigabajtach.
- Zdalny nośnik zapisany przez nowszą wersję launchera jest odrzucany, nie interpretowany.
