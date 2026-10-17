# AGENTS.md — Anweisungen für KI-/Claude-Code-Sessions

Lies diese Datei vollständig, bevor du Code oder Doku änderst. Sie ist die Kurzfassung;
die Details stehen in `development/` (lokal, nicht getrackt).

## 1. Projekt in drei Sätzen

Eine schnelle, ressourcenschonende Task-/Todo-App, die Aufgaben **offline-first** lokal hält und
**Ende-zu-Ende-verschlüsselt über Proton Drive** zwischen Geräten synchronisiert. Die UI orientiert sich
an Proton Mail/Drive (ruhig, dicht, gute Dark/Light-Unterstützung), ist aber eine eigene Umsetzung.
Reihenfolge der Plattformen: **1. Arch Linux nativ (Tauri 2)**, **2. Web**, **3. Android (später)**.

Status: **Planungs-/Skeleton-Phase.** Es gibt noch keinen Feature-Code.

## 2. Wo was liegt

| Pfad | Inhalt |
|---|---|
| `AGENTS.md` | Diese Datei – Regeln & Kontext |
| `development/` | **Alle** Architektur-, Planungs- und Entscheidungsdokumente (gitignored) |
| `crates/todo-core/` | Domain-Modell, Use-Cases, **Port-Traits** (Interfaces), Sync-Modell (HLC, Ops, Merge). Kein IO. |
| `crates/todo-sync/` | Sync-Engine (Segmente, Push/Pull, Scheduler, Snapshots). Spricht nur mit Ports. |
| `crates/todo-storage-sqlite/` | `TaskRepository`-Implementierung mit SQLite (nativ) |
| `crates/todo-remote-fs/` | `RemoteStore` auf lokalem Verzeichnis (Entwicklung, Tests, Dev-CLI) |
| `crates/proton-adapter/` | **Einziger** Ort für Proton-Code (Auth, Drive) – implementiert Ports |
| `crates/todo-wasm/` | wasm-bindgen-Bindings des Cores für die Web-App |
| `crates/todo-cli/` | Dev-CLI (`cargo run -p todo-cli -- help`): Core + SQLite ohne UI, nur über `CoreApi`; `sync --remote DIR` gegen ein lokales Verzeichnis, `login` + `sync --remote proton` gegen Proton Drive; nicht ausgeliefert |
| `crates/todo-crypto/` | `Cipher`-Implementierung (XChaCha20-Poly1305) für Sync-Segmente/Snapshots |
| `crates/todo-keystore-secret-service/` | `KeyStore` auf dem System-Schlüsselbund (Secret Service, nur Linux) |
| `apps/desktop/` | Tauri-2-Shell für Linux (dünn: Commands → Core); Rust-Crate `todo-desktop` in `src-tauri/` |
| `apps/web/` | Web-Einstieg (Vite + Svelte), Core läuft als WASM im Web Worker |
| `packages/ui/` | Gemeinsame Svelte-5-Komponenten + Design-Tokens |
| `packages/core-client/` | TS-Interface `CoreClient` + Implementierungen (Tauri-IPC / WASM-Worker) |

Detaildokumente in `development/`:
`roadmap.md` (**geordnete Arbeitsliste bis zum Prototypen – hier steht, was als Nächstes zu tun ist**),
`architecture.md`, `module-structure.md`, `interfaces.md`, `ui-guidelines.md`,
`sync-strategy.md`, `tech-decisions.md` (ADR-Log + offene Spikes), `future-android.md`.

**Session-Start:** In `development/roadmap.md` die erste offene Aufgabe (`[ ]`) suchen, ihre Vorab-Lektüre lesen,
sie vollständig abarbeiten und danach abhaken (Datum + Commit-Hash). Nicht mehrere Aufgaben vermischen.

**Vor jeder größeren Änderung** das passende Dokument lesen. Wenn `development/` fehlt
(frischer Clone – der Ordner ist nicht versioniert), den User danach fragen, statt Architektur zu raten.

## 3. Architekturprinzipien (nicht verhandelbar)

1. **Hexagonal / Ports & Adapters.** `todo-core` enthält Domain + Use-Cases + Port-Traits und hat
   **keine** Abhängigkeit auf IO, Netzwerk, UI, Tauri, wasm-bindgen oder Proton.
2. **Abhängigkeiten zeigen nach innen:**
   ```
   apps/* ──► packages/core-client ──► (IPC | WASM) ──► todo-core ◄── todo-sync
                                                          ▲   ▲
                                 todo-storage-sqlite ─────┘   └───── proton-adapter
   ```
   Adapter hängen vom Core ab, nie umgekehrt. Die Verdrahtung passiert nur in den App-Shells
   (`apps/desktop/src-tauri`, `crates/todo-wasm`).
3. **Proton-Isolation.** Proton-spezifische Typen, Endpunkte, Krypto-Details (SRP, OpenPGP, Shares/Links)
   existieren ausschließlich in `crates/proton-adapter/` (bzw. einem späteren `packages/proton-web/`).
   Nach außen nur über `AuthProvider` und `RemoteStore` (siehe `development/interfaces.md`).
   Prüfung: `rg -il proton crates/todo-core crates/todo-sync packages/ui` darf keine Code-Treffer liefern.
4. **Offline-first.** Jede Nutzeraktion wird lokal in einer Transaktion geschrieben (Zustand + Op im Outbox)
   und ist sofort sichtbar. Netzwerk passiert nur in der Sync-Engine, asynchron, nie blockierend für die UI.
5. **UI kennt nur `CoreClient`.** Svelte-Komponenten rufen nie `invoke()`, WASM oder `fetch` direkt auf.
6. **Android-fähig bleiben.** Öffentliche Core-API FFI-freundlich: owned Types, keine Lifetimes in
   Signaturen, einfache Enums/Structs, Fehler als Enum. Keine Linux-spezifischen APIs im Core.
7. **Entscheidungen dokumentieren.** Neue/geänderte Architekturentscheidung ⇒ ADR-Eintrag in
   `development/tech-decisions.md`. Nicht still vom Plan abweichen.

## 4. Performance-Regeln

Budgets (Release-Build, Arch Linux, Mittelklasse-Laptop) – Details in `development/architecture.md`:

| Metrik | Ziel | Hart |
|---|---|---|
| Kaltstart bis erste interaktive Liste (Desktop) | < 400 ms | < 800 ms |
| Idle-RAM gesamt (PSS, **inkl.** WebKit-Prozesse) | < 250 MB | < 320 MB |
| Idle-CPU | ~0 % (keine Timer/Animationen im Leerlauf) | – |
| Web: initiales JS (gzip, ohne WASM) | < 80 KB | < 150 KB |
| Web: WASM (gzip) | < 300 KB | < 600 KB |
| Interaktion (Abhaken, Tippen) bis Repaint | < 16 ms | < 50 ms |

Regeln:
- **Keine** schweren UI-Libraries (kein Material/Bootstrap/Tailwind-UI-Kits, kein Moment.js, kein Lodash-Vollimport).
  Neue npm-/Cargo-Abhängigkeit ⇒ Größe prüfen und im PR/Commit begründen.
- Listen > ~200 Einträge **virtualisieren**. Keine O(n)-Arbeit pro Tastendruck.
- IPC/WASM-Grenze grob-granular: Seiten/Diffs statt Einzelaufrufe pro Task; keine großen JSON-Blobs pro Frame.
- Sync nie auf dem UI-Thread; Tauri-Commands `async`; im Web läuft der Core in einem **Web Worker**.
- Kein Polling im Leerlauf. Sync ist ereignisgetrieben (Änderung, Fokus, Netz zurück) mit Backoff.
- Lazy-Load für selten genutzte Views (Einstellungen, Login-Flow).
- RAM immer als **PSS über alle Prozesse** messen (`tools/measure-desktop.sh` bzw. `smem -t -P 'todo|WebKit'`), nicht nur RSS des Hauptprozesses; keine zweite Instanz parallel.

Typische Fallstricke: siehe Abschnitt „Performance-Fallstricke" in `development/architecture.md`
(WebKitGTK-DMABUF/NVIDIA, Svelte-Reaktivität auf großen Arrays, `backdrop-filter`/Schatten-Overkill,
Serde-JSON über IPC, SQLite ohne WAL/Indizes, unbegrenzte Op-Logs).

## 5. Namenskonventionen

- **Rust:** Crates `kebab-case` mit Präfix `todo-` (Ausnahme: `proton-adapter`). Module/Funktionen `snake_case`,
  Typen `PascalCase`. Port-Traits benennen die Fähigkeit (`TaskRepository`, `RemoteStore`, `AuthProvider`,
  `Cipher`, `Clock`), Implementierungen das Mittel (`SqliteTaskRepository`, `ProtonDriveRemoteStore`).
- **TS/Svelte:** Dateien `kebab-case.ts`, Komponenten `PascalCase.svelte`, Stores/Runes-Module `*.svelte.ts`.
- **IDs:** UUIDv7 als String an allen Grenzen. Zeitstempel: HLC für Sync, UTC-ISO-8601 für Anzeige-Daten.
- **Domain-Sprache (verbindlich):** `Task`, `TaskList`, `Tag`, `Op`, `Device`, `Snapshot`.
  Nicht mischen mit „Todo", „Item", „Project" im Code.
- **Kein „Proton" im Produkt-/Paketnamen** (Markenrecht). „Proton" nur in Adapter-Namen.
- Commits: Conventional Commits (`feat(core): …`, `fix(sync): …`).

## 6. Kommandos

Routinen laufen über `just` (Rezepte im `justfile`, Übersicht mit `just --list`).
Die Rust-Toolchain ist über `rust-toolchain.toml` fixiert.

| Kommando | Wirkung |
|---|---|
| `just check` | **Standard-Prüfung vor jedem Commit:** `fmt-check` + `clippy` + `test` + `doc` + `wasm` + `ts` |
| `just test` | Alle Rust-Tests im Workspace (inkl. Doctests) |
| `just fmt` | Code formatieren |
| `just fmt-check` | Formatierung nur prüfen |
| `just clippy` | Clippy über alle Targets und Features, Warnungen = Fehler |
| `just doc` | API-Doku bauen, Warnungen = Fehler |
| `just wasm` | `todo-wasm` und `todo-core` (inkl. Tests) für `wasm32-unknown-unknown` prüfen |
| `just desktop-dev` | Desktop-App im Dev-Modus (Vite + Tauri-Fenster) |
| `just desktop-build` | Release-Binary der Desktop-App (`target/release/todo-desktop`) |
| `just ts` | TS/Svelte: ESLint, Typecheck, Vitest (alle pnpm-Pakete) |
| `just types` | TS-Typen der DTOs neu erzeugen (`packages/core-client/src/generated/`, läuft auch in `just test`) |

Pre-commit-Hook aktivieren (einmal pro Clone): `git config core.hooksPath .githooks`.

## 7. Arbeitsweise für Sessions

- Klein und schichtweise arbeiten: erst Core + Tests, dann Adapter, dann UI.
- Core- und Sync-Logik immer mit Unit-Tests; Merge-Logik zusätzlich mit Property-Tests (Konvergenz).
- Keine Proton-Zugangsdaten, Tokens oder Testkonten in Code, Tests oder Doku.
- Keine Code-/Asset-Kopien aus Proton-Repos (GPL-3.0 + Marke) – nur Gestaltung als Inspiration.
- Wenn eine Anforderung mit diesen Regeln kollidiert: stoppen und den User fragen.
