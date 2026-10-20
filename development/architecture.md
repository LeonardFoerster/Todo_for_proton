# Architektur (Linux + Web)

Stand: 2026-10-18 · Status: Prototyp (Desktop umgesetzt; Web-Pfad geplant, nicht implementiert)

## 1. Ziele und Nicht-Ziele

**Ziele**
- Native, schnelle Linux-App (Arch zuerst), danach Web, danach Android – mit **einer** Business-Logik.
- Offline-first: App ist ohne Netz voll benutzbar; Sync ist ein Hintergrunddetail.
- E2E-verschlüsselte Synchronisation über Proton Drive, Proton strikt hinter Interfaces.
- Proton-nahe, ruhige UI mit sauberem Dark/Light-Mode.

**Nicht-Ziele (v1)**
- Kollaboration / geteilte Listen mit anderen Personen.
- Echtzeit-Co-Editing von Notizen (Task-Notizen sind v1 Last-Writer-Wins; eigenständige Notizen im Modus „Notes“
  mergen zeilenbasiert mit Konfliktkopie, kein Text-CRDT – `notes.md`, ADR-029).
- Eigener Server. Es gibt kein Backend außer Proton Drive (ggf. reiner CORS-Proxy, siehe Risiken).
- Kalender-, Mail- oder CalDAV-Integration.

## 2. Überblick

> **Notes (N5, Entwurf):** Notizen und Ordner sind weitere Entitäten in denselben Schichten (Domain, Op-Log,
> SQLite, `CoreClient`), kein eigenes Subsystem. Erweiterung und Begründung: `notes.md` §2.

```
┌──────────────────────────────── UI-Schicht (TypeScript) ─────────────────────────────────┐
│  packages/ui  (Svelte 5 Komponenten, Design-Tokens)                                      │
│        │ nutzt ausschließlich                                                            │
│  packages/core-client  ── interface CoreClient                                           │
│        ├── TauriCoreClient   (invoke() + Events)          ← apps/desktop                 │
│        └── WorkerCoreClient  (postMessage → Web Worker)   ← apps/web   [geplant]         │
└───────────────┬───────────────────────────────────────────────┬──────────────────────────┘
                │ Tauri IPC                                     │ postMessage
┌───────────────▼──────────────┐                ┌───────────────▼───────────────────────────┐
│ apps/desktop/src-tauri        │                │ Web Worker: crates/todo-wasm              │
│ (Composition Root, nativ)     │                │ (Composition Root, wasm32) [geplant, Stub]│
└───────────────┬──────────────┘                └───────────────┬───────────────────────────┘
                └────────────────────┬──────────────────────────┘
                         ┌───────────▼────────────┐
                         │ todo-core              │  Domain · Use-Cases · Ports (Traits)
                         │ todo-sync              │  Segmente · Push/Pull · Snapshots · Scheduler
                         └──┬──────────┬───────┬──┘
              TaskRepository│  RemoteStore/Auth│  Cipher, Clock, KeyStore
          ┌─────────────────▼┐   ┌────────▼─────────┐   ┌──────────────────────┐
          │ todo-storage-     │   │ proton-adapter   │   │ Plattform-Adapter    │
          │ sqlite (nativ)    │   │ (Auth + Drive)   │   │ (Secret Service/oo7, │
          │ sqlite-wasm/OPFS  │   │                  │   │  WebCrypto, Uhr …)   │
          │ (web, s. Spike)   │   └──────────────────┘   └──────────────────────┘
          └───────────────────┘
```

### Schichten

| Schicht | Verantwortung | Darf abhängen von |
|---|---|---|
| **Domain** (`todo-core::domain`) | Entitäten, Invarianten, Wertobjekte (Task, TaskList, Tag, SortKey, Due) | nichts außer `std`, `serde`, `uuid` |
| **Application** (`todo-core::app`) | Use-Cases (`CreateTask`, `CompleteTask`, `MoveTask` …), Query-Modelle | Domain, Ports |
| **Ports** (`todo-core::ports`) | Traits: `TaskRepository`, `RemoteStore`, `AuthProvider`, `Cipher`, `KeyStore`, `Clock` | Domain |
| **Sync-Modell** (`todo-core::sync`) | HLC, Ops, Diff, Merge (ADR-015); Outbox-Buchhaltung im Repository-Commit | Domain |
| **Sync-Engine** (`todo-sync`) | Segmente, Push/Pull, Snapshots/Kompaktierung, Scheduler | Core (inkl. Ports) |
| **Adapter** | IO-Implementierungen der Ports | Core |
| **Composition Root** | Verdrahtung, Lifecycle, Plattform-Events | alles |
| **UI** | Darstellung, Interaktion, lokaler View-State | nur `CoreClient` |

## 3. Datenfluss

### Schreiben (Nutzer hakt Task ab)
1. UI ruft `core.completeTask(id, true)` (CoreClient).
2. Tauri-Command `complete_task` → `CoreApi::complete_task` → Use-Case `CompleteTask`.
3. Use-Case lädt Task, prüft Invarianten, erzeugt Domain-Änderung.
4. **Eine** Repository-Transaktion: materialisierten Zustand updaten; darin leitet `todo_core::sync::record_local`
   feldweise `Op`s mit HLC ab und schreibt sie in `outbox_ops` (ADR-015).
5. Core emittiert `CoreEvent::Changed { tasks, lists }` (Tauri-Event `core://changed`) → UI aktualisiert betroffene
   Zeilen (kein Full-Reload).
6. Der schreibende Command meldet `LocalChange` an den Scheduler; Push gebündelt nach 3 s Debounce im Hintergrund.

### Lesen
- Queries sind seitenweise (`list_tasks(TaskQueryDto, PageRequestDto)` mit Cursor), Ergebnisse sind flache DTOs.
- Die UI hält nur sichtbare + gepufferte Zeilen; Detailansicht lädt Notizen separat.

### Sync (Details: `sync-strategy.md`)
- Push: Outbox-Batch → postcard → zstd → `Cipher::seal` (XChaCha20, ADR-017) → als unveränderliche Segment-Datei
  hochladen (Proton verschlüsselt zusätzlich).
- Pull: neue Segmente anderer Geräte laden → Ops nach HLC mergen → materialisierten Zustand aktualisieren
  → `CoreEvent::Changed`.

## 4. Plattform-Umsetzung

### Linux (Priorität 1)
- **Tauri 2** mit System-WebView (WebKitGTK 4.1). UI = gleiche Svelte-App wie Web.
- Core läuft nativ im Rust-Prozess; Tauri-Commands sind dünne `async`-Wrapper um `CoreApi`.
- Storage: SQLite (`rusqlite`, gebündelt, WAL-Modus) unter `$XDG_DATA_HOME/dev.tasks.app/tasks.db`; eigenes Profil per
  `TODO_DATA_DIR` (mit eigenem Schlüsselbund-Eintrag, ADR-020).
- Secrets (Sitzung/Refresh-Token, Sync-Schlüssel): Secret Service via `oo7` (GNOME Keyring / KWallet). Ohne
  Schlüsselbund kein Fallback: Login nicht verfügbar (`AuthInfoDto.available = false`), App bleibt lokal nutzbar.
- Login: Browser-Session-Fork (ADR-013) – die Login-Seite des Anbieters erledigt Passwort, 2FA, Human Verification.
- WebKit startet ohne beschleunigten Compositing-Modus (RAM, ADR-001 Nachtrag P8-03).
- Systemintegration: `.desktop`-Datei, Tray optional (eigene Feature-Flag, kostet RAM), Benachrichtigungen
  für Fälligkeiten über `tauri-plugin-notification` (lazy) – beides nach dem Prototyp.
- Verteilung: `PKGBUILD` (AUR), später Flatpak. Abhängigkeit: `webkit2gtk-4.1`.

### Web (Priorität 2)
- Vite + Svelte 5, statisch auslieferbar.
- Core als WASM (`todo-wasm`) in einem **Dedicated Web Worker** → Main-Thread bleibt frei.
- Storage: SQLite-WASM mit OPFS im Worker (Schema-Parität zu Desktop) – **Spike S-3** entscheidet gegen IndexedDB.
- Secrets: Session-Token nur im Speicher; Refresh-Token in IndexedDB, verschlüsselt mit einem
  nicht-extrahierbaren WebCrypto-Schlüssel. Kein `localStorage` für Secrets.
- PWA: Service Worker für Offline-Shell (App startet ohne Netz).
- Risiko: Proton-API und CORS – siehe Abschnitt 7.

### Android (später) – siehe `future-android.md`.

## 5. Querschnittsthemen

- **Fehler:** Core liefert `CoreError`-Enum (`NotFound`, `Validation`, `Storage`, `AuthRequired`, `Auth`, `Network`,
  `RateLimited`, `RemoteRejected`, `Conflict`, `Internal`); an der IPC-Grenze `ErrorDto { kind, message }`. UI zeigt nur nutzerrelevante Fehler; Sync-Fehler sind still mit Statusindikator.
- **Logging:** nie Task-Inhalte oder Tokens loggen. `tracing` im `proton-adapter` (Routen-Vorlagen, ADR-018) und in
  der Desktop-Shell (Schlüsselbund, Sync-Backend, fehlgeschlagene Sync-Läufe). Die Shell installiert beim Start einen
  `tracing-subscriber` (`fmt` auf stderr, ohne Farben/`env-filter`/`log`-Brücke, +247 KB Binary); Filter über
  `TODO_LOG` in `Targets`-Syntax (Standard `info`, z. B. `TODO_LOG=info,proton_adapter::http=debug`). Seit 2026-10-18
  (N0-03). Ausnahmen auf stdout/stderr ohne `tracing`: `TODO_MEASURE …` (Messskript) und `SELFTEST …` (Dev-Selbsttest).
- **Zeit:** `Clock`-Port (testbar). HLC für Sync. Fälligkeiten als lokales Datum *oder*
  Datum+Zeit+IANA-Zone.
- **Migrationen:** versionierte SQL-Migrationen im Storage-Adapter; Sync-Format separat versioniert
  (`manifest.json` → `format_version`).
- **i18n:** geplant Deutsch + Englisch, Strings in JSON, keine schwere i18n-Library. Prototyp: UI-Texte fest auf Deutsch.
- **Tests:** Core/Sync Unit + Property-Tests (`proptest`), Adapter-Tests gegen In-Memory-Fakes der Ports,
  Proton-Adapter gegen aufgezeichnete Fixtures (keine echten Konten in CI).

## 6. Performance-Budgets und Fallstricke

### Budgets
Siehe `AGENTS.md` §4 (RAM-Budget nach Spike S-4 angehoben, Zahlen in ADR-001). Messung: `tools/measure-desktop.sh`;
- Startzeit: Zeitstempel bei `main()` und beim ersten `requestAnimationFrame` nach Render der Liste
  (per Event an Rust zurückgemeldet, im Debug-Log).
- RAM: PSS über den Prozessbaum (`/proc/*/smaps_rollup`, alternativ `smem -t -P '<binary>|WebKit'`) nach 60 s Leerlauf mit 1 000 Tasks.
- Web: Lighthouse + `vite-bundle-visualizer`; CI-Check auf Bundle-Größe.

### Abnahme Desktop (P8-03, 2026-10-18)
`tools/measure-desktop.sh` mit Release-Build, Testdatenbank 1 000 Tasks (3 Listen, 100 erledigt, über `todo-cli`
angelegt), eigenes Profil (`TODO_DATA_DIR`, nicht angemeldet), KDE Plasma Wayland, AMD Ryzen 9 5900X + Radeon (Mesa 26.2),
WebKitGTK 2.54.1. Details und Varianten: ADR-001 (Nachtrag P8-03).

| Metrik | Ergebnis | Budget | |
|---|---|---|---|
| Kaltstart bis erste interaktive Liste (Median 5 Läufe) | 382 ms (364–409) | < 400 / < 800 ms | ✔ |
| PSS gesamt nach 60 s Leerlauf | **306 MB** (App 120, WebProcess 159, Network 35) | < 250 / < 320 MB | ✔ hart, ✘ Ziel |
| Leerlauf-CPU über 5 min | 0,02 % eines Kerns (5 Ticks) | ~0 % | ✔ |
| Scrollen 1 000 Zeilen (rAF) | p50 17 / p95 18 ms, 0 Drops | 60 fps | ✔ |
| Abhaken bis Repaint | DOM 1 ms, nächster Frame 15–16 ms | < 16 / < 50 ms | ✔ |
| Binary (Release, ohne Paket) | 20 MB | – | – |

Möglich wurde das harte RAM-Budget erst durch `WEBKIT_DISABLE_COMPOSITING_MODE=1` (vorher 315–336 MB). Offen:
- Ziel < 250 MB → FIX-07 (App-Prozess: 44 MB glibc-Heap, 11 MB WebKit-Heap, Mesa/LLVM 12 MB; X11 spart 37 MB).
- Messung **angemeldet mit Sync über Proton** braucht eine Sitzung → vom User (FIX-06). Bisher (P7-11, vor dieser Änderung)
  326–330 MB, also ~20 MB über „nicht angemeldet"; Leerlauf-CPU mit Sync dort 0 %.
- NVIDIA/Intel nicht verfügbar.
- FIX-03 (O(n) beim Anlegen/Verschieben) bei 1 000 Tasks nicht messbar (< 1 ms). Anhängen seit 2026-10-18 O(1) (siehe
  Messung unten); Verschieben zwischen Nachbarn → FIX-11.

### Typische Performance-Fallstricke
**WebView / Tauri**
- WebKitGTK startet eigene Prozesse (WebProcess, NetworkProcess). Nur die Hauptprozess-RSS zu messen
  täuscht ~60–100 MB weg → immer PSS über alle Prozesse.
- NVIDIA + Wayland: DMABUF-Renderer kann weiße Fenster/Flackern verursachen
  (`WEBKIT_DISABLE_DMABUF_RENDERER=1` als dokumentierter Workaround, nicht hart einbauen).
- Große Payloads über IPC als JSON serialisieren kostet doppelt (serde + JSON.parse). Seiten/Diffs schicken;
  für Binärdaten Tauri-2-Raw-IPC nutzen.
- Synchrone Tauri-Commands blockieren den Main-Thread → immer `async fn`, schwere Arbeit in `spawn_blocking`.
- Unnötige Tauri-Plugins/Features aktiviert lassen → Binary + Startzeit wachsen.

**UI / Svelte**
- Riesige `$state`-Arrays tief reaktiv machen → `$state.raw` für Listen, gezielte Updates per ID.
- Nicht virtualisierte Listen mit tausenden DOM-Knoten.
- Teure CSS-Effekte: `backdrop-filter`, große `box-shadow`s, `filter: blur` auf scrollenden Flächen –
  WebKitGTK ist dabei deutlich langsamer als Chromium.
- Web-Fonts mit vielen Schnitten; FOIT beim Start. → System-Font-Stack oder ein einziges variables WOFF2.
- Animationen, die im Leerlauf weiterlaufen (Spinner, Pulse) → CPU-Wakeups, Akku.
- Layout-Thrashing (lesen/schreiben von Layout-Props in Schleifen).

**Daten / Sync**
- SQLite ohne WAL, ohne Indizes auf `(list_id, sort_key)`, `(status, due)`; viele Einzel-Transaktionen.
- Sortierung über Integer-Positionen → Umschreiben vieler Zeilen + Merge-Konflikte. → Fractional Indexing.
- Op-Log wächst unbegrenzt → Snapshots + Kompaktierung.
- Polling-Intervalle im Hintergrund; Retry ohne Backoff/Jitter → Rate-Limits bei Proton.
- Ganze Datenbank als eine Datei auf Drive hochladen → Konflikte, Bandbreite, Latenz.

### Messwerte Speicher (P4-04, 2026-10-17)
`cargo run --release -p todo-storage-sqlite --example bench_storage` – 10 000 Tasks in der Inbox (jede 10. erledigt),
WAL-Datei auf btrfs, AMD Ryzen 9 5900X; Median / p95 aus 200 Läufen:

| Messung | Median | p95 | Ziel |
|---|---|---|---|
| Inbox, erste Seite (50) | 0,055 ms | 0,073 ms | < 5 ms ✔ |
| Inbox offen / ab Cursor Mitte / Alle offen / Erledigt (je 50) | 0,054–0,061 ms | ≤ 0,08 ms | < 5 ms ✔ |
| `get_task` | 0,011 ms | 0,013 ms | – |
| Commit: eine Task verschieben (1 Upsert) | 0,089 ms | 0,133 ms | < 2 ms ✔ |
| 10 000 Tasks einfügen (10 Commits à 1 000) | 530 ms gesamt | – | – |
| `CoreApi::create_task` / `move_task` (Liste mit 10 000) | 6,8 / 6,5 ms | 8,3 / 7,0 ms | (16 ms Interaktion) |

Query-Pläne: Inbox/Liste lesen `tasks_by_list`, Alle `tasks_by_sort_key` bzw. `tasks_by_status_sort_key`, Erledigt
`tasks_by_status_completed` – jeweils **ohne** Sortierschritt; ein Test (`hot_queries_use_indexes_without_sorting`)
sichert das. Dafür nötig: Inbox-ID vorab auflösen (`list_id = ?` statt `IN (SELECT …)`) und die Indizes
`(status, sort_key, id)`, `(status, completed_at DESC, id)`. Heute/Demnächst sortieren ihre (kleine) Treffermenge.
Auffällig: Die Use-Cases `create_task`/`move_task` laden die Zielliste komplett (O(n), aus P3-06) → bei 10 000 Tasks
~7 ms, im Budget, aber linear wachsend → FIX-03. **Nachtrag 2026-10-18 (FIX-03/FIX-10):** Anhängen (`create_task`,
Liste löschen → Inbox) holt nur noch den größten Key (`TaskRepository::last_sort_key`, ein Zugriff
auf `tasks_by_list`): `create_task` 0,13 ms (p95 0,20 ms) statt 6,8 ms. `move_task`/`move_tasks` laden weiter die
Liste (Nachbarn, Sonderfall gleicher Keys; 6,9 ms, p95 8,3 ms) → FIX-11. Use-Cases, die eine Reihenfolge lesen und Keys vergeben, laufen über eine
gemeinsame Sperre in `Deps` nacheinander (vorher: 50 parallele `create_task` → 1 Key).

**Sammelaktionen (P0-01, 2026-10-18)** – gleiche Datenbank, je 1 000 IDs, über `CoreApi`, Median aus 4–10 Läufen:

| Messung | vorher | nachher | Ziel |
|---|---|---|---|
| `move_tasks` (1 000 IDs, Inbox mit 10 000) | 103 ms | 28 ms | < 50 ms ✔ |
| `complete_tasks` | 96 ms | 21 ms | < 50 ms ✔ |
| `delete_tasks` | 24 ms | 17 ms | < 50 ms ✔ |
| `restore_tasks` (1 000 Kopien anlegen) | 68 ms | 25 ms | < 50 ms ✔ |
| 10 000 Tasks einfügen (10 Commits à 1 000) | 645 ms | 238 ms | – |

Ursache des „vorher": Der Fremdschlüssel `tasks.list_id` zwingt SQLite zu einem Statement-Journal je Schreib-Statement,
standardmäßig als Temp-**Datei** (~45 µs je Task-Zeile). `PRAGMA temp_store = MEMORY` behebt das, ohne die Prüfung
aufzugeben. Dazu: `TaskRepository::get_tasks` (ein DB-Aufruf statt 1 000 Roundtrips, ~10 ms) und `set_entity_state`
schreibt nur geänderte Register.

### Mehrfachauswahl (P0-07, 2026-10-18)
Selbsttest `p0perf`/`p0perf10k` in der echten WebView, **Debug-Build + Vite-Dev-Server** (Svelte im Dev-Modus – also
pessimistisch gegenüber Release), gleiche Maschine wie oben; Repaint = Zeit von der Eingabe bis zum ersten Frame nach der
DOM-Änderung (MutationObserver), 20 Läufe:

| Messung | Ergebnis | Budget | |
|---|---|---|---|
| Klick schaltet um, 1 000 Tasks / 1 000 ausgewählt | p50 16–17 / p95 17–18 ms (= nächster Frame) | < 16 / < 50 ms | ✔ |
| `Umschalt+↓`, 1 000 ausgewählt | p50 16–17 / p95 17–18 ms | < 16 / < 50 ms | ✔ |
| `Strg+A` bei 1 000 (lädt 800 nach) bis Frame | 55–105 ms | – | – |
| `Strg+A` bei 10 000 (lädt 9 800 nach, 20 Seiten à 500) bis Frame | 529 ms (einmalig, inkl. 20 IPC-Aufrufe) | – | notiert |
| Klick schaltet um, 10 000 ausgewählt | p50 17 / p95 19 ms | < 16 / < 50 ms | ✔ |
| Leerlauf-CPU im Modus vs. ohne Modus (je 28 s, Prozessbaum) | 2,11 % vs. 2,03 % | ~0 % | ✔ kein Mehrverbrauch |

Der Leerlauf-Grundwert ist der Debug-/Dev-Aufbau (Release: 0,02 %, P8-03); im Modus laufen keine zusätzlichen Timer oder
Animationen. Sammelaktionen im Core: siehe „Sammelaktionen (P0-01)" oben. Große `Changed`-Events (> 20 IDs) lädt die
Liste als Seiten neu statt je ID `getTask` (500 gelöschte → 0 `getTask`); Auswahlen > 2 000 gehen in Paketen zu
`BULK_LIMIT_MAX`.

### Automatischer Upload (P1-03, 2026-10-18)
Selbsttest `p1sync` (Debug-Build, Sync gegen lokales Verzeichnis, ohne `sync.now`): eine Änderung → oben nach
**1 536 ms** (Debounce 1,5 s + Lauf); Serie von 8 Änderungen im Sekundentakt → erster Lauf nach **5 003 ms**, 2 Läufe
insgesamt (vorher: erst ~3 s nach der letzten, also nach ~11 s). Abnahme Abschnitt 6: im Eingang abgehakt → oben nach
1 520 ms, B sieht die Task unter „Erledigt".

### Listen & Board (P2-09, 2026-10-18)
Selbsttest `p2perf` (Debug-Build + Vite-Dev, pessimistisch), 10 Listen à 200 Tasks + eine Liste mit 1 000:

| Messung | Ergebnis | Budget | |
|---|---|---|---|
| Umschalten auf Kanban bis alle 12 Spalten gezeichnet (je erste Seite 200) | 74–75 ms | < 400 ms (Release) | ✔ |
| Karte per `Alt+→` in die Nachbarspalte bis Repaint (10×) | p50 16 / p95 17 ms | < 16 / < 50 ms | ✔ |
| Spalte mit 1 000 Karten durchscrollen | p50 17 / p95 17 ms, 17 DOM-Karten | 60 fps | ✔ |
| Leerlauf-CPU Board vs. Liste (je 28 s, Prozessbaum) | 1,07 % vs. 0,64–0,86 % | ~0 % | ✔ (Debug-Grundlast, ± 0,2 Pkt.) |

Im Board laufen im Leerlauf keine Timer/Animationen (nur beim Ziehen eine `requestAnimationFrame`-Schleife). Der kleine
Unterschied liegt im Rauschen der Debug-/Dev-Grundlast (Release: 0,02 %, P8-03).

**WASM**
- Große Abhängigkeiten (z. B. vollständige Krypto- oder Regex-Crates) blähen WASM auf →
  `wasm-opt -Oz`, `opt-level = "z"` nur für das WASM-Crate, Features minimieren.
- WASM auf dem Main-Thread initialisieren → Jank beim Start.

## 7. Risiken

| # | Risiko | Auswirkung | Gegenmaßnahme |
|---|---|---|---|
| R-1 | Keine offizielle, stabile Third-Party-API/OAuth für Proton; Login braucht SRP, 2FA, ggf. Human Verification (CAPTCHA) | Login-Flow aufwändig/fragil; ToS-Fragen | **Gelöst (ADR-013):** Browser-Session-Fork – Passwort/2FA/HV auf der Anbieter-Seite; ehrliche App-Version + Pflichthinweis |
| R-2 | Proton Drive SDK-Lage (offizielles SDK in TS/C#, Rust nur Community) | Rust-Adapter evtl. Eigenbau | **Umgesetzt:** Eigenbau in `proton-adapter` (ADR-013/018), Contract-Test live bestanden (P6-04) |
| R-3 | CORS: Proton-API akzeptiert evtl. keine fremden Web-Origins | Web-App kann Proton nicht direkt erreichen | Web-Sync evtl. über schlanken Same-Origin-Proxy (sieht nur verschlüsselte Daten, aber Tokens!) oder Web später ohne Sync – **offen**, Preflight-Test zu Beginn der Web-Etappe (`roadmap-next.md` N1-02) |
| R-4 | WebKitGTK-Performance/Bugs je nach GPU-Treiber | Ruckeln, Rendering-Fehler | CSS schlank halten; Fallback GTK4-UI möglich, da Core UI-unabhängig ist |
| R-5 | Proton-API-Änderungen | Sync bricht | Isolation im Adapter, Contract-Tests, Format-Versionierung |
