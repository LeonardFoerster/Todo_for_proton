# Modul- und Ordnerstruktur

Stand: Prototyp (2026-10-18, P8-04) – Ist-Zustand; geplante Teile sind markiert.

## 1. Struktur

```
proton_todo/
├── AGENTS.md                     # Regeln für KI-Sessions (Kurzfassung)
├── CLAUDE.md                     # importiert AGENTS.md
├── Cargo.toml                    # Cargo-Workspace
├── package.json                  # pnpm-Root (nur Scripts/DevDeps)
├── pnpm-workspace.yaml
├── justfile                      # Entwickler-Kommandos (just check/test/fmt/clippy/doc/wasm/ts/types)
├── rust-toolchain.toml, rustfmt.toml, .editorconfig
├── .cargo/config.toml            # TS_RS_EXPORT_DIR für ts-rs (ADR-019)
├── .githooks/pre-commit          # just check
├── development/                  # Planung & ADRs – gitignored (archive/: Spike-Quellen)
│
├── crates/
│   ├── todo-core/                # Domain + Use-Cases + Ports. KEIN IO.
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── domain/           # task.rs, task_list.rs, sort_key.rs, due.rs, ids.rs, timestamp.rs, text.rs
│   │       ├── app/              # Use-Cases: create_task, update_task, complete_task, reopen_task, move_task, delete_task,
│   │       │                     # create/rename/delete_task_list, ensure_inbox (je eine Datei)
│   │       ├── ports/            # repository.rs, remote_store.rs, auth.rs, cipher.rs (Cipher + KeyStore), clock.rs
│   │       ├── sync/             # hlc.rs, op.rs, diff.rs, merge.rs, replica.rs (reines Sync-Modell, ADR-015)
│   │       ├── testing/          # Fakes + Contract-Suiten (Feature test-support)
│   │       ├── api/              # mod.rs (CoreApi-Fassade, CoreEvent), dto.rs (DTOs, ts-rs)
│   │       └── error.rs          # CoreError
│   │
│   ├── todo-sync/                # Sync-Engine, nur gegen Ports
│   │   ├── src/
│   │   │   ├── segment.rs        # Segment-Format (40-Byte-Header, Cipher), HLC/Ops/Merge liegen in todo-core (ADR-015)
│   │   │   ├── compression.rs    # zstd (nativ) bzw. ruzstd (wasm32)
│   │   │   ├── snapshot.rs
│   │   │   ├── engine.rs         # push/pull-Zyklus, Bootstrap, Kompaktierung
│   │   │   └── scheduler.rs      # Debounce, Backoff, Trigger, SyncStatus (runtime-frei)
│   │   └── tests/                # engine, scenarios (P5-09), snapshots, first_login (P8-01, ADR-020); common/, golden/
│   │
│   ├── todo-storage-sqlite/      # impl TaskRepository (rusqlite, WAL, Migrationen)
│   │   ├── migrations/           # 0001_init.sql, 0002_sync.sql
│   │   ├── examples/             # bench_storage.rs (P4-04, Release-Messung)
│   │   └── src/                  # connection.rs (DB-Thread), migrations.rs, mapping.rs, query.rs, repository.rs,
│   │                             # sync_store.rs (Outbox, Register, Cursor), error.rs
│   │
│   ├── todo-remote-fs/           # impl RemoteStore auf lokalem Verzeichnis (Dev, Tests, Dev-CLI)
│   ├── todo-crypto/              # impl Cipher: XChaCha20-Poly1305 (RustCrypto), nativ + wasm32
│   ├── todo-keystore-secret-service/ # impl KeyStore: freedesktop Secret Service via oo7 (nur Linux, tokio)
│   │
│   ├── todo-cli/                 # Dev-CLI über CoreApi (nicht ausgeliefert)
│   │
│   ├── proton-adapter/           # EINZIGER Ort für Proton-Code
│   │   └── src/
│   │       ├── auth/             # fork.rs (Session-Fork), session.rs (Tokens, Refresh), provider.rs → impl AuthProvider
│   │       ├── drive/            # crypto.rs (Schlüsselkette, Knoten, Blöcke), mapping.rs (Proton-DTOs → Port-Typen),
│   │       │                     # store.rs → impl RemoteStore; fake.rs/tests.rs (Fake-Server), live.rs (#[ignore])
│   │       ├── api_client.rs     # HTTP, Header, Timeouts, Storage-Blöcke, Logging mit Routen-Vorlagen
│   │       ├── error.rs          # ApiError → CoreError
│   │       └── secret.rs         # redigierter Geheimnis-Typ
│   │
│   └── todo-wasm/                # Composition Root für Web – bisher nur Stub (lib.rs), wasm-bindgen folgt (N1)
│       └── src/lib.rs
│
├── apps/
│   ├── desktop/                  # Tauri 2
│   │   ├── src/                  # main.ts (mountet packages/ui mit TauriCoreClient), selftest.ts (Dev-Selbsttest)
│   │   └── src-tauri/            # Rust Composition Root: lib.rs, commands.rs, state.rs, events.rs, sync.rs
│   │                             # (Scheduler-Treiber), dto.rs, dev.rs (nur Debug: Verzeichnis-Backend, Selbsttest-Steuerung);
│   │                             # build.rs (ACL-Manifest), capabilities/, tests/commands.rs (IPC-Test, MockRuntime)
│   └── web/                      # Vite-SPA – noch nicht angelegt (nur README), geplant: main.ts, core.worker.ts
│
├── packages/
│   ├── core-client/              # TS: interface CoreClient + DTO-Typen + Implementierungen
│   │   └── src/
│   │       ├── core-client.ts    # CoreClient, AuthClient, SyncClient
│   │       ├── generated/        # DTO-Typen aus Rust (ts-rs, `just types`, ADR-019)
│   │       ├── types.ts          # re-exportiert generated/
│   │       ├── errors.ts         # CoreClientError { kind }
│   │       ├── fake-core-client.ts (+ .test.ts)  # In-Memory für UI-Tests/Entwicklung
│   │       ├── tauri-core-client.ts # Subpath @tasks/core-client/tauri (+ devSelftest)
│   │       └── index.ts          # (geplant: worker-core-client.ts für die Web-App)
│   └── ui/                       # Svelte-5-Komponenten + Design-Tokens
│       └── src/
│           ├── tokens/           # tokens.css (Farben, Abstände, Radien, Typo)
│           ├── theme/            # Theme-Umschaltung + Vite-Plugin fürs Theme-Snippet im <head>
│           ├── components/       # Button, IconButton, Checkbox, TextInput, Textarea, Menu, Dialog, Toaster
│           │                     # (+ toasts.svelte.ts), ThemeSwitch, VirtualList
│           ├── features/         # task-list/, task-detail/, sidebar/, new-task/, account/ (Einstellungen + Login),
│           │                     # shell/ (Kürzel-Übersicht)
│           ├── lib/              # router.svelte.ts, shortcuts.ts, views.ts, lists/sync.svelte.ts, task-actions.ts,
│           │                     # context.ts, local-day.ts, preferences.ts, format-date.ts
│           ├── icons.ts          # Lucide-Icons einzeln
│           └── App.svelte        # (demo/ neben src/: Komponenten-Demo, nur Dev)
│
└── tools/                        # measure-desktop.sh (Budgets), acceptance-two-devices.sh (P8-02); später Bundle-Check, PKGBUILD
```

Nicht angelegt, bis gebraucht: `apps/android/`, `packages/proton-web/`, `apps/desktop-gtk/`.

## 2. Abhängigkeitsregeln

| Modul | darf importieren | darf **nicht** importieren |
|---|---|---|
| `todo-core` | `serde`, `uuid` (nur `v7`), `thiserror`, `chrono` (ohne Default-Features), `async-trait`, `postcard` (Ops, ADR-016), `futures` (optional, `test-support`); dev: `proptest`, `serde_json`, `ts-rs` (TS-Typen, ADR-019) | tokio, rusqlite, reqwest, tauri, wasm-bindgen, alles Proton |
| `todo-sync` | `todo-core`, `serde`/`serde_json`, `postcard`, `futures`, `zstd` (Feature `zstd-native`) bzw. `ruzstd` (`pure-rust-zstd`), Krypto nur über `Cipher`-Port; dev: Adapter-Crates für Tests | Adapter-Crates (außer dev), Runtime-Crates |
| `todo-crypto` | `todo-core` (Ports), `chacha20poly1305` (ohne Default-Features), `getrandom` (gleiche Version wie `uuid`) | alles andere |
| `todo-storage-sqlite` | `todo-core`, `rusqlite` (`bundled`), `async-trait`, `futures` (nur `oneshot`), `chrono` (nur `Datelike`); dev: `tempfile` | `todo-sync`-Interna, Proton, Async-Runtimes |
| `todo-keystore-secret-service` | `todo-core` (Ports), `async-trait`, `oo7` (nur Linux; `tokio` + `native_crypto`) | Proton, Sync, Storage |
| `todo-remote-fs` | `todo-core` (Ports), `async-trait`; dev: `tempfile` | `todo-sync`, Storage, Proton |
| `todo-cli` (Dev-Werkzeug, Composition Root) | `todo-core` (`api`; Ports nur zum Verdrahten), `todo-storage-sqlite`, `todo-sync`, `todo-remote-fs`, `todo-crypto`, `proton-adapter` (nur öffentliche Adapter-Typen, seit P6-05), `todo-keystore-secret-service` (Linux), `tokio` (`rt`, `time`; Single-Thread-Runtime), `async-trait` | Proton-Interna (Endpunkte, Krypto), Use-Cases/Repository direkt |
| `proton-adapter` | `todo-core` (Ports), `reqwest`/`rustls`(ring), `tokio` (nur `sync`), `proton-crypto`/`proton-crypto-account` (`rustpgp`), `aes-gcm`, `hmac`/`sha1`/`sha2`, `base64`, `hex`, `getrandom`, `chrono`, `tracing`; dev: `wiremock`, `tokio`, `tracing-subscriber`, `futures` | `todo-sync`, Storage, UI, KeyStore-Implementierungen (bekommt den Port injiziert) |
| `todo-wasm`, `src-tauri` (Crate `todo-desktop`) | alles (Composition Root); `src-tauri`: `tauri`, `tokio` (`sync`, `time`, `macros`), `serde`, `async-trait`, Adapter-Crates (inkl. `todo-remote-fs` für das Debug-Backend); dev: `ts-rs`, `tauri/test`, `tempfile`, `serde_json` | Use-Cases/Repository direkt (nur über `CoreApi`), Proton-Interna |
| `packages/ui` | `packages/core-client` (nur Interface + Typen), Svelte | `@tauri-apps/*`, WASM, `fetch` zu Proton |
| `packages/core-client` | `@tauri-apps/api` (nur in `tauri-core-client.ts`) | Svelte, UI |

Durchsetzung:
- Rust: Abhängigkeiten in `Cargo.toml` sind die Wahrheit – ein Blick in `[dependencies]` von `todo-core`
  muss genügen. Optional später `cargo-deny`/eigener CI-Check.
- TS: ESLint `no-restricted-imports` in `packages/ui` gegen `@tauri-apps/*`.

## 3. Feature-Ordner in der UI

Feature-orientiert, nicht nach Typ:

```
features/task-list/
├── TaskList.svelte          # Container: holt Daten über CoreClient
├── TaskRow.svelte           # reine Darstellung, Props rein / Events raus
├── view-match.ts            # Ansichtsprädikat (spiegelt TaskQuery::matches)
└── TaskList.test.ts, sort-delete.test.ts, view-match.test.ts
```

Regel: Container-Komponenten sprechen mit `CoreClient`; Präsentationskomponenten bekommen nur Props.

```
features/notes/                 # Modus „Notes" (N5), Lazy-Chunk – Modus selbst in lib/app-mode.svelte.ts
├── index.ts                    # Einstieg des Chunks (NotesState, NotesSidebar, NotesMain)
├── notes-state.svelte.ts       # Zustand: Baum, offene Notiz, Papierkorb, Import, Konflikt-Hinweise
├── tree-model.ts               # Baum flach machen (virtualisiert), Tastatur-Cursor
├── NotesSidebar.svelte, NoteTree.svelte, NoteTreeRow.svelte, FolderPicker.svelte, NoteTrash.svelte
├── NotesMain.svelte            # Editor-Bereich (Zen, Schließen-Protokoll)
├── NoteEditor.svelte, NoteStatusBar.svelte, editor-commands.ts   # Textarea + reine Editor-Befehle
├── ImportResultDialog.svelte   # Ergebnis von „Ordner importieren…" (nur Desktop)
├── notes-actions.ts            # Papierkorb mit Rückgängig u. Ä.
└── markdown/                   # render.ts (eigener Renderer, nur DOM, keine HTML-Durchreichung), MarkdownPreview.svelte
```

Rust-Seite: `todo-core` `domain/note*.rs`, `app/notes.rs`, `sync/{revision,text_merge}.rs`,
`ports/note_repository.rs`, `api/note_dto.rs`; Desktop-Shell `src-tauri/src/import.rs` (Ordner-Import).

## 4. Namenskonventionen (Ergänzung zu AGENTS.md)

- Use-Case-Dateien = Verb + Objekt: `complete_task.rs`, `move_task.rs`.
- Tauri-Commands = `snake_case` gleichnamig zum Use-Case: `complete_task`.
- `CoreClient`-Methoden = `camelCase` gleichnamig: `completeTask`.
- Events: `core://changed`, `core://sync-status` (Tauri) bzw. gleiche Namen als Worker-Message-Typ.
- SQL: Tabellen Plural `snake_case` (`tasks`, `task_lists`, `outbox_ops`), Spalten `snake_case`.
- Tests: Rust `#[cfg(test)] mod tests` im Modul + `tests/` für Integration; TS `*.test.ts` neben der Datei.
