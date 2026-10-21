# Zentrale Interfaces (Ports)

Alle Ports liegen in `crates/todo-core/src/ports/`. Implementierungen liegen in Adapter-Crates.
Stand: Prototyp (2026-10-18). Die Blöcke geben die tatsächlichen Signaturen wieder (gekürzt); Grenzen sind verbindlich.

## 0. Gemeinsame Konventionen

```rust
// Async-Traits müssen auf wasm32 ohne Send laufen (Browser ist single-threaded),
// nativ aber Send sein (tokio/Tauri). Muster für jeden Port:
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait TaskRepository: MaybeSend + MaybeSync { /* … */ }
// MaybeSend/MaybeSync = Send/Sync nativ, leere Marker-Traits auf wasm32.
// In den Beispielen unten abgekürzt als `#[port]` – bewusst kein Makro (ports/mod.rs), die zwei cfg_attr-Zeilen stehen ausgeschrieben.

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("not found: {0}")]       NotFound(String),
    #[error("validation: {0}")]      Validation(String),
    #[error("storage: {0}")]         Storage(String),
    #[error("auth required")]        AuthRequired,
    #[error("auth: {0}")]            Auth(AuthError),
    #[error("network: {0}")]         Network(String),   // retrybar
    #[error("rate limited")]         RateLimited { retry_after_s: u32 },   // retrybar nach Wartezeit (P5-06)
    #[error("remote rejected: {0}")] RemoteRejected(String), // nicht retrybar
    #[error("conflict: {0}")]        Conflict(String),
    #[error("internal: {0}")]        Internal(String),
}
impl CoreError { pub fn is_retryable(&self) -> bool; }   // Network, RateLimited
```

Regeln:
- Ports verwenden **nur Domain-/Port-Typen**. Kein `reqwest::Error`, kein `rusqlite::Error`, keine Proton-Structs.
- Adapter mappen ihre Fehler auf `CoreError`, inkl. Einordnung „retrybar" (`Network`) vs. „endgültig".
- IDs sind Newtypes über `Uuid` (v7): `TaskId`, `TaskListId`, `TagId`, `DeviceId`.

## 1. Domain-Kern (Stand nach P1, `crates/todo-core/src/domain/`)

Entities haben **private Felder** mit Gettern; Änderungen nur über Methoden, die Invarianten prüfen.
Mutierende Methoden nehmen die Uhr als `&(impl Clock + ?Sized)` (passt auch für `&dyn Clock`) und setzen
`updated_at` **nur, wenn sich wirklich etwas ändert** (keine leeren Sync-Ops).

```rust
// ids.rs – Newtypes über UUIDv7, serde als String; nicht untereinander austauschbar
pub struct TaskId(Uuid);      // ebenso TaskListId, TagId, DeviceId
impl TaskId { fn new() -> Self; fn parse(&str) -> Result<Self, ParseIdError>; fn to_bytes(self) -> [u8; 16]; fn from_bytes([u8; 16]) -> Self }
// + Display, FromStr. Feste Inbox-ID auf allen Geräten: TaskListId::INBOX (…0001, ADR-020)
// ParseIdError → CoreError::Validation (From-Impl)

// timestamp.rs – Unix-ms UTC, ohne Kalender-Crate; serde als Zahl
pub struct Timestamp(i64);    // from_millis, as_millis, plus_millis (sättigend), EPOCH

// ports/clock.rs
pub trait Clock { fn now(&self) -> Timestamp; }
// FixedClock, SteppingClock: nur mit cfg(test) bzw. Feature `test-support`

pub struct Task {             // Felder privat, Getter gleichen Namens
    id: TaskId,
    list_id: TaskListId,
    parent_id: Option<TaskId>,     // Subtasks (max. 1 Ebene in v1) – noch ohne Logik
    title: String,                 // getrimmt, nicht leer, max. 500 Zeichen (chars, nicht Bytes)
    notes: String,                 // Markdown-light, LWW in v1
    status: TaskStatus,            // Open | Done
    priority: Priority,            // None < Low < Medium < High
    due: Option<Due>,
    tags: Vec<TagId>,              // noch ohne Logik
    sort_key: SortKey,
    created_at: Timestamp,
    updated_at: Timestamp,
    completed_at: Option<Timestamp>,
}
impl Task {
    fn new(list_id, title: &str, sort_key, clock) -> CoreResult<Task>;
    fn rename(&mut self, title: &str, clock) -> CoreResult<()>;
    fn set_notes(&mut self, notes: &str, clock);
    fn set_priority(&mut self, Priority, clock);
    fn set_due(&mut self, Option<Due>, clock) -> CoreResult<()>;   // validiert tz
    fn complete(&mut self, clock);   // idempotent, behält erstes completed_at
    fn reopen(&mut self, clock);     // löscht completed_at
    fn move_to(&mut self, list_id, sort_key, clock);   // Liste und/oder Position
}

// due.rs – Datums-Crate: chrono (default-features = false, nur serde)
pub enum Due {
    Date(NaiveDate),                                  // „am 20.10."
    DateTime { at: Timestamp, tz: String },           // „20.10. 14:00 Europe/Berlin" – tz nur gespeichert, ≤ 64 Zeichen
}
impl Due {
    fn is_overdue(&self, today: NaiveDate, now: Timestamp) -> bool;            // Date: erst ab Folgetag
    fn is_due_on(&self, day: NaiveDate, day_span: &Range<Timestamp>) -> bool;  // Aufrufer liefert lokale Tagesgrenzen
    fn is_due_by(&self, day: &LocalDay) -> bool;                                // „Heute" inkl. Überfälliger (TaskQuery::matches)
}
pub struct LocalDay { date: NaiveDate, start: Timestamp, end: Timestamp }      // lokaler Tag, von der Shell berechnet (new prüft start < end)

// sort_key.rs – Fractional Index nach Greenspan: Ganzzahlteil variabler Länge + Bruchteil, Base62,
// Byte-Vergleich (SQLite: BINARY-Collation)
pub struct SortKey(String);
impl SortKey {
    fn first() -> SortKey;                                              // "a0"
    fn between(a: Option<&SortKey>, b: Option<&SortKey>) -> CoreResult<SortKey>;  // Fehler, wenn nicht a < b
    fn n_between(a: Option<&SortKey>, b: Option<&SortKey>, n: usize) -> CoreResult<Vec<SortKey>>; // P0-01, Halbierung
    fn parse(&str) -> CoreResult<SortKey>;                              // Formatprüfung für Speicher/Sync
}

pub struct TaskList {         // Felder privat, Getter gleichen Namens
    id: TaskListId,
    name: String,                  // wie Titel, max. 100 Zeichen
    sort_key: SortKey,
    created_at: Timestamp,
    updated_at: Timestamp,
    is_inbox: bool,                // genau eine Inbox; Durchsetzung im Use-Case
}
impl TaskList {
    fn new(name: &str, sort_key, clock) -> CoreResult<TaskList>;
    fn inbox(sort_key, clock) -> TaskList;     // Name = INBOX_NAME ("Inbox"), UI übersetzt
    fn rename(&mut self, name: &str, clock) -> CoreResult<()>;
    fn can_delete(&self) -> bool;              // false für Inbox
    fn can_rename(&self) -> bool;              // false für Inbox
}
```

Bekannte, bewusste Abweichung von „kein IO im Core": `TaskId::new()` usw. lesen über `uuid` direkt Systemzeit
und Zufall (nicht über `Clock`). IDs müssen nicht deterministisch sein; im Browser liefert die Composition Root
`todo-wasm` die Zufallsquelle (`uuid`-Feature `js`).

Wiederherstellen aus dem Speicher (P4-03): `TaskParts` / `TaskListParts` (alle Felder `pub`) und
`Task::from_parts(parts) -> CoreResult<Task>` bzw. `TaskList::from_parts` – prüfen dieselben Invarianten wie die
Eingabe (Titel/Name, Zeitzone) plus „`completed_at` genau dann gesetzt, wenn `Done`". Nur für Adapter/Sync gedacht,
nicht für die UI.

Noch offen (nach dem Prototyp): `Tag`, Subtask-Regeln. Ops werden mit postcard serialisiert (ADR-016), Entities nicht.

## 2. TaskRepository (lokaler Speicher)

Stand P5-05 (`crates/todo-core/src/ports/repository.rs`). Sync-Buchhaltung läuft in denselben Transaktionen
(ADR-015): Repositories rufen `todo_core::sync::record_local` / `sync::apply_remote` über den Trait `ReplicaStore` auf.

```rust
pub trait TaskRepository: MaybeSend + MaybeSync {          // async-trait, ?Send auf wasm32
    async fn get_task(&self, id: TaskId) -> CoreResult<Option<Task>>;
    async fn get_tasks(&self, ids: &[TaskId]) -> CoreResult<Vec<Task>>;    // P0-01: unbekannte entfallen; Default = Schleife
    async fn get_task_list(&self, id: TaskListId) -> CoreResult<Option<TaskList>>;
    async fn list_task_lists(&self) -> CoreResult<Vec<TaskList>>;          // sort_key, id
    async fn list_tasks(&self, query: &TaskQuery, page: PageRequest) -> CoreResult<Page<TaskSummary>>;
    async fn last_sort_key(&self, list_id: TaskListId) -> CoreResult<Option<SortKey>>; // FIX-03: Anhängen; Default = alle Seiten
    /// Atomar: Zustand + Ops (Outbox) + Register + HLC. Änderungen an Tombstones entfallen.
    async fn commit(&self, changes: ChangeSet) -> CoreResult<CommitReceipt>;

    // --- Sync (P5-05) ---
    async fn device_id(&self) -> CoreResult<DeviceId>;                     // beim ersten Öffnen erzeugt
    async fn pending_ops(&self, limit: u32) -> CoreResult<Vec<Op>>;        // seq aufsteigend
    async fn pending_op_count(&self) -> CoreResult<u64>;
    async fn mark_ops_pushed(&self, up_to: u64) -> CoreResult<()>;         // löscht seq <= up_to
    async fn apply_remote(&self, batch: RemoteBatch) -> CoreResult<RemoteReceipt>;  // Merge + Cursor, atomar
    async fn sync_cursor(&self, device: DeviceId) -> CoreResult<Option<u64>>;
    async fn set_sync_cursor(&self, device: DeviceId, seq: u64) -> CoreResult<()>;
    async fn sync_cursors(&self) -> CoreResult<Vec<(DeviceId, u64)>>;      // alle Cursor (Gerätedatei, Snapshot-Vektor)
    async fn entity_state(&self, entity: EntityRef) -> CoreResult<Option<EntityState>>;  // Register = „Field-Clocks"
    async fn entity_states(&self) -> CoreResult<Vec<(EntityRef, EntityState)>>;          // alle, für Snapshots (P5-11)
    async fn sync_value(&self, key: &str) -> CoreResult<Option<String>>;   // KV der Engine
    async fn set_sync_value(&self, key: &str, value: String) -> CoreResult<()>;
}

pub struct RemoteBatch { pub ops: Vec<Op>, pub cursors: Vec<(DeviceId, u64)>, pub now: Timestamp }
pub struct RemoteReceipt { pub changed: Vec<EntityRef>, pub clock_skews: u32 }
// ChangeSet::at(now): Zeit für die HLC; setzt der CoreApi-Decorator, sonst spätestes updated_at.
// CoreApi::repository(): das dekorierte Repository – die Sync-Engine muss es nutzen (Events für Remote-Änderungen).
pub enum EntityRef { Task(TaskId), TaskList(TaskListId) }
pub enum Change { UpsertTask(Task), UpsertList(TaskList), Delete(EntityRef) }  // Delete unbekannter IDs = No-op
pub struct ChangeSet { /* Vec<Change>, Builder: upsert_task/upsert_list/delete, entities() */ }
pub struct CommitReceipt { pub changed: Vec<EntityRef> }

pub enum SmartView { Inbox, List(TaskListId), Today(LocalDay), Upcoming(LocalDay), All, Completed }
pub struct TaskQuery { pub view: SmartView, pub status: Option<TaskStatus>, pub tag: Option<TagId> }
// TaskQuery::matches(&task, &list) definiert die Semantik; Sortierung: Completed nach completed_at desc,
// Today/Upcoming nach Due::sort_instant(&day) asc, dann sort_key (ADR-026), sonst sort_key asc; Tiebreak id.
pub struct LocalDay { date: NaiveDate, start: Timestamp, end: Timestamp }   // vom Aufrufer (lokale Zone)
pub struct PageRequest { pub cursor: Option<String>, pub limit: u32 }      // 1..=500, Cursor opak
pub struct SearchQuery { pub text: String, pub limit: u32 }                // search_tasks (ADR-027), Standard O(n), SQLite FTS5
pub struct Page<T>     { pub items: Vec<T>, pub next_cursor: Option<String> }
pub struct TaskSummary { id, list_id, title, status, priority, due, has_notes, sort_key, completed_at }
```

Abweichungen vom ursprünglichen Entwurf: `list` steckt in `SmartView::List`; `due_before` entfällt (Today/Upcoming
mit `LocalDay` decken das ab, ohne Zonenrechnung im Core); `search` entfällt im Prototyp (keine Volltextsuche);
`get_task_list` neu; IDs per Wert (Copy) statt Referenz.

Regeln für `commit`: Nach Anwendung muss jede Task auf eine existierende Liste verweisen, sonst
`Validation` und keine Änderung. Upsert ersetzt die Entity vollständig.

Implementierungen: `SqliteTaskRepository` (nativ, `crates/todo-storage-sqlite`: `open(path)`, `open_in_memory()`;
eigener DB-Thread, ADR-014), Web-Speicher (S-3), `InMemoryTaskRepository` (Tests).

## 3. AuthProvider (Login, Proton-frei formuliert)

Stand P6-03 (`crates/todo-core/src/ports/auth.rs`, Impl. `ProtonAuthProvider`): Login per **Browser-Session-Fork**. Die App sieht weder Passwort noch 2FA, FIDO2,
Two-Password-Mode oder Human Verification – das erledigt die Login-Seite des Anbieters im Browser. Für die App ist
der Login daher zweistufig: „URL öffnen" → „warten, bis bestätigt".

```rust
#[port]
pub trait AuthProvider {
    async fn current_session(&self) -> CoreResult<Option<SessionInfo>>;
    /// Startet einen Login. Liefert die zu öffnende URL (enthält einen Einmal-Schlüssel – nie loggen).
    async fn begin_login(&self) -> CoreResult<AuthStep>;
    /// Fragt den Login-Status ab (ein Aufruf, kein Blockieren). Der Aufrufer pollt im genannten Intervall.
    async fn poll_login(&self, flow: &AuthFlowId) -> CoreResult<AuthStep>;
    async fn cancel_login(&self, flow: &AuthFlowId) -> CoreResult<()>;
    /// Stellt eine gespeicherte Session wieder her / erneuert Tokens. Kein UI-Kontakt.
    async fn resume(&self) -> CoreResult<Option<SessionInfo>>;
    async fn logout(&self) -> CoreResult<()>;
}

pub enum AuthStep {
    /// Browser öffnen; danach alle `poll_interval_s` `poll_login` aufrufen, bis `expires_in_s` abläuft.
    OpenBrowser { flow: AuthFlowId, url: String, poll_interval_s: u32, expires_in_s: u32 },
    /// Noch nicht im Browser bestätigt.
    Pending { flow: AuthFlowId },
    Done(SessionInfo),
}

pub struct SessionInfo { pub account_label: String, pub account_id: String }

pub enum AuthError {
    Expired,                          // Login-Fenster abgelaufen
    Cancelled,                        // abgebrochen oder unbekannter Flow
    SessionRevoked,                   // Refresh endgültig abgelehnt (Proton HTTP 422 / 10013) → neu anmelden
    ClientRejected(String),           // App-Version fehlt/ungültig/veraltet (Proton 5002/5003, 2064, 400/2000)
    Unsupported(String),
}
// Drosselung ist CoreError::RateLimited (nicht AuthError); fehlende Anmeldung CoreError::AuthRequired.
```

Semantik (P6-03):
- `current_session()` liest nur den lokalen Stand (KeyStore), kein Netz.
- `resume()`: Refresh, wenn Netz da ist; **ohne Netz bleibt die gespeicherte Sitzung gültig** (offline-first);
  `Auth(SessionRevoked)` nur, wenn der Server den Refresh-Token ablehnt – dann ist die Sitzung lokal gelöscht.
- `logout()` meldet serverseitig ab (best effort) und löscht lokal immer.
- Abweichung vom Roadmap-Entwurf: kein `continue_login` (TOTP, zweites Passwort, HV) – erledigt die Login-Seite im Browser.
- Adapter-intern: authentifizierte Aufrufe erneuern bei 401 einmal (serialisiert, parallele Aufrufer warten auf denselben
  Refresh), danach `AuthRequired`. Der Scheduler pausiert bei `AuthRequired`/`Auth(_)` (`Paused(AuthRequired)`, kein Timer)
  bis `SetEnabled(true)`.

- Refresh rotiert den Refresh-Token und macht den alten Access-Token sofort ungültig (P2-03 live) → Refresh
  serialisieren (nie parallel), neue Tokens sofort persistieren.
- UID, Refresh-Token und das Schlüssel-Passwort aus dem Fork speichert der Adapter als ein JSON-Eintrag `proton-session`
  über den `KeyStore`-Port; der Access-Token bleibt nur im Speicher. Nichts davon verlässt den Adapter.
- Die UI rendert den Login generisch anhand von `AuthStep`; sie weiß nicht, dass Proton dahintersteht.
  Branding-/Hinweistexte kommen aus der Composition Root, inkl. Pflichthinweis
  „This is a third-party application not officially supported by Proton." (Proton-SDK-Regeln, siehe ADR-013).
- Plan B (nicht implementiert): SRP-Login mit `proton-srp`; dann kämen `NeedSecondFactor`/`NeedHumanVerification`-Schritte hinzu.

## 4. RemoteStore (Drive/Sync-Backend)

Absichtlich **schmal** und dateiorientiert: Die Sync-Engine braucht nur einen App-Ordner mit
unveränderlichen Dateien. Alles Proton-Spezifische (Volumes, Shares, Links, Node-Keys, Revisionen,
Block-Upload) bleibt im Adapter.

```rust
#[port]
pub trait RemoteStore {
    /// Stellt sicher, dass der App-Ordner existiert, und liefert seinen Handle.
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir>;

    async fn list(&self, dir: &RemotePath, since: Option<&RemoteCursor>) -> CoreResult<RemoteListing>;
    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>>;

    /// Schreibt eine NEUE Datei. Schlägt mit Conflict fehl, wenn sie existiert (keine Überschreibung).
    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry>;

    /// Nur für kleine, gerätelokale Metadateien (z. B. devices/<id>.json). Optimistic Concurrency.
    async fn replace(&self, path: &RemotePath, bytes: Vec<u8>, expected: Option<&Revision>) -> CoreResult<RemoteEntry>;

    async fn delete(&self, path: &RemotePath) -> CoreResult<()>;

    /// Änderungs-Feed des Backends (Proton Drive Volume-Events). None = Backend kann das nicht.
    /// Bei Proton PFLICHT-Weg für die Erkennung fremder Änderungen: periodisches `list` ist laut
    /// Proton-Regeln untersagt („do not poll … use Drive events"); `list` nur für Bootstrap/Reparatur.
    async fn changes_since(&self, cursor: Option<&RemoteCursor>) -> CoreResult<Option<RemoteChanges>>;
}

pub struct RemoteEntry   { pub path: RemotePath, pub size: u64, pub revision: Revision, pub modified: Timestamp }
pub struct RemoteListing { pub entries: Vec<RemoteEntry>, pub cursor: RemoteCursor }
pub struct RemoteChanges { pub changed: Vec<RemoteEntry>, pub deleted: Vec<RemotePath>, pub cursor: RemoteCursor,
                           pub refresh: bool }   // refresh: Lücke im Feed → einmal komplett listen
pub struct RemoteDir { pub path: RemotePath }    // App-Ordner; join(name) → Pfad darin
```

Proton-Abbildung (Stand P2, Details `development/spikes/proton-research.md` §6/§12):
- `create` → Datei anlegen + Block-Upload + Revision committen; Name existiert → Proton-Code 2500 → `CoreError::Conflict`.
- `replace` → neue Revision derselben Datei; `Revision` = Proton-Revisions-ID; Abweichung → `Conflict`.
- `delete` → `trash_multiple` (Papierkorb) + endgültig über den Papierkorb-Endpunkt (`…/trash/delete_multiple`);
  `folders/{p}/delete_multiple` gilt nur für Entwürfe (P2-05: 2501).
- `changes_since` → Volume-Events v1 (`/drive/volumes/{v}/events/{id}`: Events mit vollständigem Link, `More`,
  `Refresh` = Vollabgleich nötig). Kosten: Upload ≈ 1,7 s, Download ≈ 0,9 s je kleiner Datei (P2-05) → Push bündeln.
- Dateien klein halten (ein Block ≤ 4 MiB; der SDK-Weg `files/small` deckt < 128 KiB in einem Request ab).
- Fehler: HTTP 429/`Retry-After` und Netzfehler → `Network` (retrybar, Backoff); 4xx sonst → `RemoteRejected`.

Stand P6-04 (`crates/proton-adapter/src/drive/`): `ProtonDriveRemoteStore::new(&ProtonAuthProvider)` teilt die Sitzung;
App-Ordner-Pfade liegen unter „Meine Dateien" (Haupt-Share). Tatsächliche Abbildung:
- Auflösung Pfad → Link über entschlüsselte Namen; Ordner (Schlüssel) und Ordnerinhalte im Speicher-Cache, verworfen bei
  jedem Schreiben in den Ordner, bei Events des Ordners und vor jedem „nicht gefunden" (einmal frisch nachsehen).
- `list` lädt immer frisch (rekursiv). `RemoteEntry.size` ist bei `list`/Events die **verschlüsselte** Speichergröße
  (Klartextgröße nur im verschlüsselten XAttr), bei `create`/`replace` die Klartextgröße – die Engine nutzt `size` nicht.
- `create`: Entwurf (`POST /drive/shares/{s}/files`, mit `ClientUID`) → Verifikation (v2) → `POST /drive/blocks` →
  Storage-Upload (4-MiB-Blöcke) → Commit (v2 `PUT …/revisions/{r}`). 2500 mit reinem Entwurf (`ConflictDraftRevisionID`,
  keine aktive Revision) = liegengebliebener Upload → Entwurf löschen, einmal neu; sonst `Conflict`. Scheitert der Upload,
  wird der eigene Entwurf gelöscht (harmlos, falls der Commit doch durchging → späteres `create` = `Conflict`).
- `replace`: neue Revision mit `CurrentRevisionID` (Server prüft mit; 409/2500 → `Conflict`), offener eigener Entwurf
  wird einmal verworfen; danach die vorige Revision löschen (best effort), damit Metadateien keine Historie ansammeln.
- `delete`: `trash_multiple` + `v2/…/trash/delete_multiple` (Name wird frei); Einzelcodes 2501 = schon weg.
- `changes_since`: v1-Volume-Events (`More`/`Refresh` als 0/1, max. 20 Seiten je Aufruf). Lösch-Events tragen nur die
  `LinkID` → Pfad aus dem Cache bekannter Dateien. Events für Entwürfe werden nachgeladen (Commit-Event nicht garantiert).
  Unbekannte Elternordner werden über die Elternkette aufgelöst; nur Pfade unter geöffneten App-Ordnern zählen.
  Unbekannter Cursor → `refresh`.
- Signaturprüfung beim Lesen (ADR-025): Share- und Node-Passphrasen, Namen und Block-Manifest gegen die Adress-Keys,
  `NodeHashKey`/`ContentKeyPacket` gegen den Node-Key, Block-Hashes gegen das Manifest; fail closed. Grenzen: Dateien
  ≤ 128 MiB, keine Weiterleitungen, Storage nur `https://*.proton.me`. Die Sync-Segmente sind zusätzlich mit dem
  App-Schlüssel authentisiert (AEAD), Metadateien (`manifest.json`, `devices/*.json`) nur über Drive.
- Später: `files/small` (1 statt ~5 Requests je Upload) als Optimierung.

Implementierungen: `ProtonDriveRemoteStore` (`proton-adapter`), `FsRemoteStore` (lokales Verzeichnis –
Entwicklung & Integrationstests), `InMemoryRemoteStore` (Unit-Tests, inkl. Fehler-/Latenz-Injektion).

> **Notes (N5, Entwurf):** `RemoteStore` bleibt für Notizen unverändert – sie sind nur neue Op-Inhalte. Neuer
> Lese-Port `NoteRepository` und erweitertes `ChangeSet`: `notes.md` §2/§3.5/§7.

## 5. Cipher + KeyStore (Encryption)

Stand P5-07 (`crates/todo-core/src/ports/cipher.rs`, ADR-017):

```rust
pub struct SecretBytes(Vec<u8>);   // Debug zeigt nie den Inhalt

/// Symmetrische AEAD-Verschlüsselung der Sync-Payloads (zusätzlich zu Protons E2E).
pub trait Cipher: MaybeSend + MaybeSync {
    fn generate_key(&self) -> CoreResult<SecretBytes>;
    fn seal(&self, key: &SecretBytes, plaintext: &[u8], aad: &[u8]) -> CoreResult<Vec<u8>>;   // nonce || ciphertext || tag
    fn open(&self, key: &SecretBytes, sealed: &[u8], aad: &[u8]) -> CoreResult<Vec<u8>>;     // Validation bei Manipulation
}

#[port]
pub trait KeyStore {
    async fn get(&self, name: &str) -> CoreResult<Option<SecretBytes>>;
    async fn put(&self, name: &str, value: SecretBytes) -> CoreResult<()>;
    async fn delete(&self, name: &str) -> CoreResult<()>;
}
```

Abweichung vom Entwurf: Schlüsselmaterial statt `KeyId` (die Cipher bleibt zustandslos; die Engine holt den
Schlüssel aus dem `KeyStore`), `generate_key` an der Cipher (sie besitzt die Zufallsquelle).

- `Cipher` ist synchron (CPU-Arbeit, kleine Payloads). Standard-Impl: `XChaCha20Poly1305Cipher` (RustCrypto)
  im Crate `todo-crypto`.
- AAD = kompletter 40-Byte-Segment-Header (`magic ‖ format_version ‖ codec ‖ reserviert(2) ‖ device_id ‖ seq_from ‖ seq_to`)
  → verhindert Vertauschen/Wiederverwenden von Segmenten.
- Sync-Schlüssel: zufällig beim ersten Gerät erzeugt, als verschlüsselte Datei `keys/sync-key.enc` im
  App-Ordner abgelegt (durch Protons E2E geschützt); weitere Geräte laden ihn nach Login.
- `KeyStore`-Impls: `SecretServiceKeyStore` (Linux, oo7), `InMemoryKeyStore` (Tests), `ProcessKeyStore` (Dev-CLI und
  Debug-Backend der Desktop-App, nur im Prozess); geplant: `WebCryptoKeyStore` (Web), `AndroidKeyStore`.

## 6. Clock

```rust
pub trait Clock: MaybeSend + MaybeSync {
    fn now(&self) -> Timestamp;   // UTC, ms
}
```
Für Tests `FixedClock`/`SteppingClock`. Die HLC (`todo_core::sync::hlc`, ADR-015) basiert auf `Clock`.

**TimeZones** (N4, ADR-028): `to_local(at, tz) -> Option<NaiveDateTime>`, `to_utc(local, tz) -> Option<Timestamp>`
(Lücke → später, doppelt → früher). Optional über `CoreApi::with_time_zones`; ohne Port lehnen Serien **mit Uhrzeit**
mit `Validation` ab, ganztägige gehen immer. Desktop: `JiffZones` (`apps/desktop/src-tauri/src/zones.rs`, System-tzdb);
Tests: `FixedOffsetZones`.

## 7. Fassade für UI-Shells (`todo_core::api`) und `CoreClient` (TS)

Stand P3-07 (`crates/todo-core/src/api/`). Nur DTOs (`api::dto`), IDs als `String`, Zeitpunkte UTC-ISO-8601
(`…T12:00:00.000Z`), Daten `YYYY-MM-DD`; alle DTOs `serde` mit camelCase.

```rust
pub struct CoreApi { /* Deps (Arc<dyn TaskRepository>, Arc<dyn Clock>), Sinks */ }
impl CoreApi {
    pub fn new(repo: Arc<dyn TaskRepository>, clock: Arc<dyn Clock>) -> Self;
    pub fn subscribe(&self, sink: Box<dyn EventSink>);          // Arc<T: EventSink> ist ebenfalls EventSink
    pub fn repository(&self) -> Arc<dyn TaskRepository>;        // dekoriert (Events) – für die Sync-Engine
    // Listen
    pub async fn ensure_inbox(&self) -> CoreResult<TaskListDto>;  // beim Start
    pub async fn list_task_lists(&self) -> CoreResult<Vec<TaskListDto>>;
    pub async fn create_task_list(&self, name: String) -> CoreResult<TaskListDto>;
    pub async fn rename_task_list(&self, id: String, name: String) -> CoreResult<TaskListDto>;
    pub async fn delete_task_list(&self, id: String) -> CoreResult<u32>;   // Anzahl in die Inbox verschobener Tasks
    pub async fn move_task_list(&self, id: String, target: MoveListTargetDto) -> CoreResult<TaskListDto>; // P2-01, ADR-023
    // list_task_lists: Inbox immer zuerst, dann sort_key
    // Tasks
    pub async fn create_task(&self, input: CreateTaskDto) -> CoreResult<TaskDto>;      // { title, listId?, notes?, priority?, due? } – ein Commit
    pub async fn get_task(&self, id: String) -> CoreResult<TaskDto>;
    pub async fn update_task(&self, id: String, patch: TaskPatchDto) -> CoreResult<TaskDto>; // due: null = entfernen
    pub async fn complete_task(&self, id: String, done: bool) -> CoreResult<TaskDto>;
    pub async fn move_task(&self, id: String, target: MoveTargetDto) -> CoreResult<TaskDto>;  // { listId?, after?, before? }
    pub async fn delete_task(&self, id: String) -> CoreResult<()>;
    // Sammelaktionen (P0-01, ADR-021): je ein Commit + ein Changed-Event; unbekannte IDs übersprungen,
    // Ergebnis = tatsächlich geänderte Tasks; höchstens BULK_LIMIT_MAX (2 000) je Aufruf.
    pub async fn complete_tasks(&self, ids: Vec<String>, done: bool) -> CoreResult<Vec<TaskDto>>;
    pub async fn delete_tasks(&self, ids: Vec<String>) -> CoreResult<Vec<TaskDto>>;       // Zustand vor dem Löschen
    pub async fn move_tasks(&self, ids: Vec<String>, target: MoveTargetDto) -> CoreResult<Vec<TaskDto>>; // Block
    pub async fn restore_tasks(&self, tasks: Vec<TaskDto>) -> CoreResult<Vec<TaskDto>>;  // neue IDs, alter Sort-Key
    // Papierkorb (ADR-031): gleiche IDs, nur Ansicht `Trash` zeigt sie; endgültig nur aus dem Papierkorb
    pub async fn trash_tasks(&self, ids: Vec<String>) -> CoreResult<Vec<TaskDto>>;
    pub async fn untrash_tasks(&self, ids: Vec<String>) -> CoreResult<Vec<TaskDto>>;
    pub async fn purge_tasks(&self, ids: Vec<String>) -> CoreResult<u32>;
    pub async fn empty_task_trash(&self) -> CoreResult<u32>;
    pub async fn purge_expired_tasks(&self) -> CoreResult<u32>;          // > 30 Tage, Shell höchstens täglich
    pub async fn set_dues(&self, dues: Vec<TaskDueDto>) -> CoreResult<Vec<TaskDto>>;     // je Task eigener Wert (ADR-026)
    pub async fn search_tasks(&self, text: String, limit: u32) -> CoreResult<Vec<TaskSummaryDto>>; // ADR-027
    pub async fn list_tasks(&self, query: TaskQueryDto, page: PageRequestDto) -> CoreResult<PageDto<TaskSummaryDto>>;
    // Notizen (N5-06, ADR-029; DTOs in `api::note_dto`). Elemente im Papierkorb sind nur lesbar.
    pub async fn list_note_folders(&self) -> CoreResult<Vec<NoteFolderDto>>;              // alle, auch Papierkorb
    pub async fn list_note_items(&self, folder: Option<String>, page: PageRequestDto) -> CoreResult<PageDto<NoteTreeItemDto>>;
    pub async fn get_note(&self, id: String) -> CoreResult<NoteDto>;
    pub async fn create_note(&self, folder: Option<String>, kind: NoteKindDto) -> CoreResult<NoteDto>;
    pub async fn update_note_body(&self, id: String, text: String, base_rev: Option<String>) -> CoreResult<NoteDto>;
    pub async fn discard_empty_note(&self, id: String) -> CoreResult<bool>;
    pub async fn rename_note(&self, id: String, name: String) -> CoreResult<NoteDto>;    // Auto-Name aus
    pub async fn set_note_kind(&self, id: String, kind: NoteKindDto) -> CoreResult<NoteDto>;
    pub async fn move_note(&self, id: String, target: NoteTargetDto) -> CoreResult<NoteDto>;
    pub async fn create_note_folder(&self, parent: Option<String>, name: String) -> CoreResult<NoteFolderDto>;
    pub async fn rename_note_folder(&self, id: String, name: String) -> CoreResult<NoteFolderDto>;
    pub async fn move_note_folder(&self, id: String, target: NoteTargetDto) -> CoreResult<NoteFolderDto>;
    pub async fn trash_note_items(&self, items: Vec<NoteItemDto>) -> CoreResult<Vec<NoteItemDto>>;   // geänderte
    pub async fn restore_note_items(&self, items: Vec<NoteItemDto>) -> CoreResult<Vec<NoteItemDto>>;
    pub async fn purge_note_items(&self, items: Vec<NoteItemDto>) -> CoreResult<u32>;    // samt Nachfahren
    pub async fn empty_note_trash(&self) -> CoreResult<u32>;
    pub async fn purge_expired_notes(&self) -> CoreResult<u32>;                          // > 30 Tage
    pub async fn list_note_trash(&self) -> CoreResult<Vec<NoteTrashEntryDto>>;          // oberste, neueste zuerst
    pub async fn search_notes(&self, text: String, limit: u32) -> CoreResult<Vec<NoteHitDto>>; // Pfad + Ausschnitt
    pub async fn import_notes(&self, root: String, files: Vec<ImportNoteFileDto>) -> CoreResult<ImportReportDto>;
}
// NoteTreeItemDto (tag = "type"): folder{…NoteFolderDto} | note{…NoteSummaryDto}   (kein Inhalt)
// NoteItemDto (tag = "type"): note{id} | folder{id};  NoteTargetDto { folderId?, after?, before? }
// NoteDto.bodyRev "<gerät>.<nummer>" → beim nächsten Speichern als baseRev (je Notiz nacheinander speichern)
// NoteHitDto { note, path, nameHit, snippet?{ text, offset, length } } – offset/length in UTF-16 (Editor)

pub enum CoreEvent {
    Changed { tasks: Vec<String>, lists: Vec<String>, notes: Vec<String>, note_folders: Vec<String> }, // genau die committeten Entities
    NoteConflicts { conflicts: Vec<NoteConflictDto> },   // N5-06: beim Zusammenlegen entstandene Konfliktkopien
}
pub trait EventSink: MaybeSend + MaybeSync { fn emit(&self, event: CoreEvent); }

// ViewDto (tag = "kind"): inbox | list{listId} | today{day} | upcoming{day} | all | completed | trash
// TaskDto/TaskSummaryDto.trashedAt: string | null (ADR-031)
// LocalDayDto { date: "YYYY-MM-DD", start: ISO, end: ISO }   – Shell berechnet in lokaler Zone
// DueDto (tag = "kind"): date{date} | dateTime{at, tz}
```

Events entstehen in einem Decorator um das Repository (jeder erfolgreiche `commit` → `Changed`); Use-Cases
kennen keine Events. Abweichung vom Entwurf: `Changed` trennt Tasks und Listen (Sidebar vs. Liste),
`complete_task` liefert die Task zurück. Sync-Status und Login laufen nicht über `CoreApi`, sondern über die Shell
(Scheduler-Treiber bzw. `AuthProvider` in der Composition Root).

Desktop-Shell (P7-03, `apps/desktop/src-tauri`): Commands `snake_case` gleichnamig zu `CoreApi` (Argumente in JS
camelCase, z. B. `invoke("complete_task", { id, done })`), dazu `auth_info`, `auth_current`, `auth_begin_login`
(öffnet den Browser selbst per `xdg-open`), `auth_open_browser(flow)`, `auth_poll_login(flow)`, `auth_cancel_login(flow)`,
`auth_logout`, `sync_status`, `sync_now`, `sync_flush` (N5-06: sofort hochladen, wenn etwas wartet – Notiz-/Moduswechsel),
`set_online(online)`, `app_ready` (erster Frame → Startzeit + Scheduler `AppStarted` + Auto-Leeren des
Notiz-Papierkorbs, danach höchstens täglich bei Fokus); Notiz-Commands gleichnamig zu `CoreApi`
(`update_note_body` meldet dem Scheduler `ContentChange` statt `LocalChange`), dazu `import_notes_folder` (N5-11:
Ordner über das XDG-Desktop-Portal wählen, in der Shell einlesen, `import_notes` aufrufen; `null` bei Abbruch) und
`open_external(url)` (nur http/https/mailto); nur Debug-Builds wirksam: `dev_selftest` (Suiten aus `TASKS_SELFTEST`), `dev_selftest_report(line, exit)`
(stdout, optional beenden) – in `TauriCoreClient` als `devSelftest`, nicht Teil von `CoreClient`. Events: `core://changed` (`ChangedEventDto { tasks, lists, notes, noteFolders }`), `core://note-conflicts`
(`NoteConflictDto[] { noteId, copyId }`), `core://sync-status` (`SyncStatusDto
{ state, lastSuccess, pendingOps, message }`, `state` ∈ disabled|idle|syncing|offline|authRequired|rateLimited|
newerFormat|error). Fehler: `ErrorDto { kind, message }`. Login-Schritte: `AuthStepDto` (tag `kind`: openBrowser{flow,
url, pollIntervalS, expiresInS} | pending{flow} | done{session}); `AuthInfoDto { provider, notice, available }` liefert
Anbietername + Pflichthinweis aus der Composition Root.

```ts
// TS: packages/core-client/src/core-client.ts (P7-04) – gleiche Methoden in camelCase; Ablehnung immer als
// CoreClientError { kind: ErrorKind, message }. Implementierungen: TauriCoreClient (Subpath `@tasks/core-client/tauri`),
// FakeCoreClient (In-Memory, Core-Semantik inkl. Inbox-ID, Validierung, Reihenfolge, Ansichten, Seiten, Events).
export interface CoreClient {
  ensureInbox(): Promise<TaskListDto>;
  listTaskLists(): Promise<TaskListDto[]>;
  createTaskList(name: string): Promise<TaskListDto>;
  renameTaskList(id: string, name: string): Promise<TaskListDto>;
  moveTaskList(id: string, target: MoveListTargetDto): Promise<TaskListDto>;  // P2-02, Eingang bleibt vorn
  deleteTaskList(id: string): Promise<number>;
  createTask(input: CreateTaskDto): Promise<TaskDto>;
  getTask(id: string): Promise<TaskDto>;
  updateTask(id: string, patch: TaskPatchDto): Promise<TaskDto>;
  completeTask(id: string, done: boolean): Promise<TaskDto>;
  moveTask(id: string, target: MoveTargetDto): Promise<TaskDto>;
  deleteTask(id: string): Promise<void>;
  listTasks(query: TaskQueryDto, page: PageRequestDto): Promise<PageDto<TaskSummaryDto>>;
  // P0-02, ADR-021: ein Aufruf = ein Commit = ein onChange; unbekannte IDs übersprungen
  // N4, ADR-028: Serie bleibt offen, rückt ab `today` vor; Verlaufseintrag (deterministische ID) in `tasks` hinten
  completeTasks(ids: string[], done: boolean, today?: LocalDayDto): Promise<CompleteResultDto>; // {tasks, occurrences}
  skipOccurrence(id: string, today?: LocalDayDto): Promise<TaskDto>;  // nur vorrücken, kein Verlauf
  undoOccurrence(o: OccurrenceDto): Promise<TaskDto>;                  // {seriesId, occurrenceId|null, previousDue}
  deleteTasks(ids: string[]): Promise<TaskDto[]>;                    // Zustand vor dem Löschen
  moveTasks(ids: string[], target: MoveTargetDto): Promise<TaskDto[]>; // Block in Anzeigereihenfolge
  restoreTasks(tasks: TaskDto[]): Promise<TaskDto[]>;                // neue IDs, alter Sort-Key
  trashTasks(ids: string[]): Promise<TaskDto[]>;                      // Papierkorb (ADR-031), gleiche IDs
  untrashTasks(ids: string[]): Promise<TaskDto[]>;
  purgeTasks(ids: string[]): Promise<number>;                         // nur aus dem Papierkorb
  emptyTaskTrash(): Promise<number>;
  data?: DataClient;          // nur Desktop (ADR-032): exportAll(today) → ExportResultDto | null, importFile() → ImportDataReportDto | null
  background?: BackgroundClient; // nur Desktop (ADR-033): get/set(BackgroundSettingsDto), onShellAction(openTask | newTask)
  setDues(dues: TaskDueDto[]): Promise<TaskDto[]>;                   // {id, due|null} je Task (ADR-026)
  searchTasks(text: string, limit: number): Promise<TaskSummaryDto[]>; // Titel + Notiz, Teilwort (ADR-027)
  notes: NotesClient; // N5-06: listFolders, listItems, get, create, updateBody(id, text, baseRev), discardIfEmpty,
                      // rename, setKind, move, createFolder, renameFolder, moveFolder, trash, restore, purge,
                      // emptyTrash, listTrash, search, onConflict; optional importFolder() (nur Desktop,
                      // N5-11: Ordnerwahl im System-Dialog → ImportReportDto | null bei Abbruch)
  auth: AuthClient;   // info, current, beginLogin, openBrowser(flow), pollLogin(flow), cancelLogin(flow), logout
  sync: SyncClient;   // status, now, flush
  ready(): Promise<void>;                                 // erste Ansicht des Modus gerendert (app_ready)
  onChange(cb: (e: ChangedEventDto) => void): () => void;      // gibt Unsubscribe zurück
  onSyncStatus(cb: (s: SyncStatusDto) => void): () => void;
}
// Nur TauriCoreClient (nicht im Interface): setOnline(online) – Netzwechsel der WebView an den Scheduler.
// Nur FakeCoreClient: notes.simulateRemoteBody(id, text), notes.simulateConflict(conflicts) für UI-Tests.
```

DTO-Typen werden aus Rust generiert (`ts-rs` als dev-dependency, ADR-019) nach `packages/core-client/src/generated/`.
