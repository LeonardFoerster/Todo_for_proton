# Roadmap zum ersten funktionsfähigen Prototypen

Das ist **keine** Zeitplan-Roadmap, sondern eine **geordnete Arbeitsliste**. Sie wird von oben nach unten
abgearbeitet; jede Aufgabe ist so geschnitten, dass sie in einer Session (oder wenigen) erledigt werden kann.

## Wie diese Liste benutzt wird

- **Immer die erste offene Aufgabe** nehmen (`[ ]`). Nicht springen, außer eine Aufgabe sagt es ausdrücklich.
- Vor dem Start: Abschnitt „Abhängig von" prüfen und die genannten Dokumente lesen.
- Eine Aufgabe ist erst fertig, wenn **alle** Punkte unter „Fertig wenn" erfüllt sind – inkl. grüner Tests.
- Danach: Checkbox auf `[x]` setzen, Datum + Commit-Hash dahinter schreiben, z. B. `[x] … — 2026-10-20 · a1b2c3d`.
- Abweichungen vom Plan: kurz unter der Aufgabe als `> Notiz:` festhalten; größere Entscheidungen als ADR in
  `tech-decisions.md`.
- Ein Commit pro Aufgabe (Conventional Commits, Scope = Crate/Paket), sofern nicht anders angegeben.
- Standard-Prüfung nach jeder Rust-Aufgabe: `just check` (ab P0-03; davor `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`).

## Ziel: Was „Prototyp fertig" bedeutet

Auf **zwei Arch-Linux-Rechnern** (oder zwei Nutzerprofilen auf einem Rechner) läuft die Desktop-App.
Man meldet sich mit einem Proton-Konto an, legt Tasks an, bearbeitet, hakt ab, sortiert und löscht sie –
auch offline. Änderungen erscheinen nach dem Sync auf dem jeweils anderen Gerät, gleichzeitige Offline-Änderungen
werden korrekt zusammengeführt. Die Performance-Budgets aus `AGENTS.md` §4 werden eingehalten.

**Nicht** im Prototyp: Web-App, Android, Tags-UI, Subtasks-UI, Volltextsuche, Benachrichtigungen, Packaging.

## Phasenübersicht

| Phase | Inhalt | Warum an dieser Stelle |
|---|---|---|
| P0 | Werkzeuge & Repo-Hygiene | Grundlage für alles |
| P1 | Domain-Modell mit einfachen Unit-Tests | Kleinster, risikoloser Einstieg; testet die Arbeitsweise |
| P2 | **Proton-Machbarkeit** (Spike) | Größtes Projektrisiko früh klären, bevor viel darauf gebaut wird |
| P3 | Ports, Fakes, Use-Cases | Anwendungslogik komplett testbar ohne IO |
| P4 | SQLite-Speicher | Echte lokale Persistenz |
| P5 | Sync-Engine gegen Fakes | Kernlogik von Offline-First, vollständig ohne Proton testbar |
| P6 | Proton-Adapter (produktiv) | Spike-Erkenntnisse sauber hinter die Ports bringen |
| P7 | Desktop-App (Tauri + Svelte) | UI auf stabilem Fundament |
| P8 | Integration & Abnahme | Alles zusammen, zwei Geräte, Messungen |

---

## P0 · Werkzeuge & Repo-Hygiene

### [x] P0-01 · Toolchain fixieren — 2026-10-17 · 2c537fa
**Ziel:** Jeder (Mensch oder Agent) baut mit derselben Toolchain.
**Schritte:**
1. `rust-toolchain.toml` im Root anlegen: aktuelle stabile Version, Komponenten `rustfmt`, `clippy`,
   Target `wasm32-unknown-unknown` (für später, schadet nicht).
2. `rustfmt.toml` minimal (z. B. `max_width = 100`, `imports_granularity = "Crate"` nur wenn stable-fähig – sonst weglassen).
3. `.editorconfig` (UTF-8, LF, 4 Spaces Rust, 2 Spaces TS/Svelte/JSON/YAML).
**Fertig wenn:** `cargo --version` im Repo nutzt die fixierte Version; `cargo fmt --check` läuft durch.
> Notiz: Fixiert auf exakt `1.97.1` (statt `stable`), damit Builds reproduzierbar bleiben. `rustfmt.toml` nur mit
> `edition` + `max_width = 100`; `imports_granularity` ist weiterhin nightly-only und wurde weggelassen.

### [x] P0-02 · Workspace-Lints — 2026-10-17 · 2c537fa
**Ziel:** Einheitliche Code-Qualität ab der ersten Zeile.
**Schritte:**
1. In `Cargo.toml`: `[workspace.lints.rust]` (`unsafe_code = "forbid"`, `missing_debug_implementations = "warn"`)
   und `[workspace.lints.clippy]` (`all = "warn"`, ausgewählte `pedantic`-Lints, nicht pauschal).
2. In jedem Crate `[lints] workspace = true`.
**Fertig wenn:** `cargo clippy --all-targets -- -D warnings` ist grün.
> Notiz: Zusätzlich `rust_2018_idioms`, `unused_qualifications` und `clippy::dbg_macro`. Wirksamkeit per absichtlichem
> Verstoß geprüft. Risiko für später: `unsafe_code = "forbid"` könnte mit wasm-bindgen-Makros in `todo-wasm`
> kollidieren – falls ja, dort gezielt auf `deny` herabsetzen und als ADR festhalten.

### [x] P0-03 · `justfile` mit Standard-Kommandos — 2026-10-17 · 2c537fa
**Ziel:** Ein Befehl pro Routine; AGENTS.md kann darauf verweisen.
**Schritte:**
1. `just` installieren (`pacman -S just`), `justfile` anlegen mit:
   `check` (fmt-check + clippy + test), `test`, `fmt`, `clippy`.
2. In `AGENTS.md` einen Abschnitt „Kommandos" ergänzen.
**Fertig wenn:** `just check` läuft grün.
> Notiz: `pacman -S just` braucht sudo; stattdessen benutzerlokal via `cargo install just --locked` (1.59.0) installiert.
> Zusätzliches Rezept `fmt-check`. Abschnitt „Kommandos" ist jetzt AGENTS.md §6 (Arbeitsweise → §7).

### [x] P0-04 · Erster Commit + minimale CI (lokal) — 2026-10-17 · 2c537fa
**Ziel:** Sauberer Ausgangspunkt in Git.
**Schritte:**
1. Pre-commit-Hook optional (`.githooks/pre-commit` → `just check`, aktivieren mit `git config core.hooksPath .githooks`).
2. Initialen Commit erstellen.
3. CI (GitHub/Codeberg) erst, wenn ein Remote existiert – dann Workflow, der `just check` ausführt.
**Fertig wenn:** `git log` zeigt den Initial-Commit; `git status` sauber.
> Notiz: Da das Repo noch keinen Commit hatte, sind P0-01…P0-04 gemeinsam im Initial-Commit `2c537fa` gelandet.
> Pre-commit-Hook aktiv (`core.hooksPath = .githooks`). CI entfällt vorerst – noch kein Remote vorhanden.

---

## P1 · Domain-Modell mit einfachen Tests (`crates/todo-core`)

Lies vorher: `interfaces.md` §0–1, `module-structure.md`.
Grundregel P1: **Test zuerst** schreiben, dann Implementierung. Keine IO, keine async.

### [x] P1-01 · Erster Smoke-Test & Modulgerüst — 2026-10-17 · f226943
**Ziel:** Testinfrastruktur funktioniert; Modulstruktur steht.
**Schritte:**
1. Module anlegen: `domain/mod.rs`, `error.rs`, `ports/mod.rs`, `app/mod.rs` (leer bis auf Doku-Kommentar).
2. Ein trivialer Test in `lib.rs` (`it_compiles`) – nur um `cargo test -p todo-core` zu verifizieren.
**Fertig wenn:** `cargo test -p todo-core` zeigt 1 grünen Test.

### [x] P1-02 · ID-Newtypes — 2026-10-17 · c38f0b9
**Ziel:** Typsichere IDs (`TaskId`, `TaskListId`, `TagId`, `DeviceId`).
**Schritte:**
1. Abhängigkeiten: `uuid` (Features `v7`, `serde`), `serde` (`derive`).
2. Ein Makro oder generischer Typ für Newtypes über `Uuid`: `new()` (v7), `parse(&str)`, `Display`, `Serialize/Deserialize` als String.
3. Tests: Roundtrip String ↔ ID; zwei neue IDs sind verschieden; v7-IDs sind zeitlich sortierbar; ungültiger String → Fehler.
**Fertig wenn:** Tests grün; IDs kompilieren nicht untereinander vertauschbar (`TaskId` ≠ `TaskListId`).
> Notiz: IDs serialisieren explizit als String (eigenes serde-Impl), daher `uuid` nur mit Feature `v7`, ohne `serde`.
> Parse-Fehler als eigener `ParseIdError` (kein `uuid`-Typ nach außen). Nicht-Vertauschbarkeit per `compile_fail`-Doctest;
> dabei aufgefallen: `cargo test --all-targets` lässt Doctests aus → `just test` hat einen zweiten Lauf `--doc` (eigener Commit).

### [x] P1-03 · `CoreError` und `Timestamp` — 2026-10-17 · 7de685b
**Ziel:** Gemeinsame Fehler- und Zeittypen.
**Schritte:**
1. `thiserror` hinzufügen, `CoreError` gemäß `interfaces.md` §0 (Varianten, die schon gebraucht werden; `Auth` später).
2. `Timestamp` als Newtype über `i64` (Unix-ms, UTC) mit `Ord`, `serde`. Bewusst **ohne** chrono im Domain-Kern,
   falls möglich; Kalenderdaten in P1-05.
3. `Clock`-Trait (`fn now(&self) -> Timestamp`) + `FixedClock` und `SteppingClock` (nur unter `#[cfg(any(test, feature = "test-support"))]`).
**Fertig wenn:** Tests für Ordnung von `Timestamp` und für `SteppingClock` grün.
> Notiz: `CoreError` ohne `Auth`/`AuthRequired` (kommen mit dem Auth-Port). `Clock` liegt in `ports/clock.rs`; Test-Uhren
> nutzen `AtomicI64`, damit sie später `Sync` erfüllen. `just clippy` läuft jetzt mit `--all-features`.

### [x] P1-04 · `Task`-Entity mit Invarianten — 2026-10-17 · 28113ab
**Ziel:** Kern-Entity mit Validierung.
**Schritte:**
1. `Task`, `TaskStatus` (`Open`, `Done`), `Priority` (`None`, `Low`, `Medium`, `High`) gemäß `interfaces.md` §1
   (ohne `tags`/`parent_id`-Logik – Felder dürfen schon existieren).
2. Konstruktor `Task::new(list_id, title, sort_key, clock)` validiert: Titel getrimmt, nicht leer, ≤ 500 Zeichen.
3. Methoden: `rename`, `complete(clock)` (setzt `completed_at`), `reopen` (löscht `completed_at`), `set_notes`,
   `set_priority`. Jede Methode setzt `updated_at`.
4. Tests je Methode inkl. Fehlerfälle (leerer Titel, nur Whitespace, 501 Zeichen, Unicode-Titel zählt Zeichen statt Bytes).
**Fertig wenn:** Alle Tests grün; keine `pub`-Felder, die Invarianten umgehen (Felder lesbar über Getter oder `pub(crate)`).
> Notiz: Alle mutierenden Methoden (auch `reopen`) nehmen `&(impl Clock + ?Sized)`, damit `updated_at` gesetzt werden kann
> und auch `&dyn Clock` passt. Methoden sind no-ops, wenn sich nichts ändert (kein `updated_at`-Sprung → später keine
> leeren Ops); `complete` auf erledigter Task behält das erste `completed_at`. `SortKey` existiert vorerst nur als
> Newtype (Konstruktion in P1-06), `due` folgt in P1-05. Validierung als `domain/text.rs::normalize_name` für P1-07 wiederverwendbar.

### [x] P1-05 · `Due` (Fälligkeit) — 2026-10-17 · 3270e62
**Ziel:** Datum oder Datum+Zeit+Zone korrekt modellieren.
**Schritte:**
1. Entscheidung: `time` oder `chrono` für `NaiveDate` – kleinere Abhängigkeit wählen, im Commit begründen.
2. `enum Due { Date(NaiveDate), DateTime { at: Timestamp, tz: String } }` + `Task::set_due(Option<Due>)`.
3. Hilfsfunktion `Due::is_overdue(today: NaiveDate, now: Timestamp) -> bool` und `is_due_on(day)`.
4. Tests: überfällig gestern/heute/morgen, Zeitzonen-String wird nur gespeichert (keine Zonenberechnung im Core in v1).
**Fertig wenn:** Tests grün.
> Notiz: `chrono` (ohne Default-Features) statt `time`: WASM-Differenz gemessen < 1 KB gzip, aber 1 statt 4 transitive Deps.
> Abweichung: `is_due_on(day, day_span: &Range<Timestamp>)` – für `DateTime` liefert der Aufrufer die lokalen Tagesgrenzen,
> weil der Core keine Zonen rechnet. `Task::set_due` validiert den Zonennamen (nicht leer, ≤ 64 Zeichen) und ist fehlbar.

### [x] P1-06 · `SortKey` (Fractional Indexing) — 2026-10-17 · 06e4fed
**Ziel:** Umsortieren ändert nur eine Task (Grundlage für konfliktarmen Sync).
**Schritte:**
1. `SortKey(String)` mit Base62-Alphabet; Funktionen `SortKey::first()`, `between(a: Option<&SortKey>, b: Option<&SortKey>)`.
2. Implementierung selbst schreiben (klein, gut testbar) oder ein winziges Crate prüfen – Größe/Lizenz notieren.
3. Tests: `between(a,b)` liegt strikt zwischen a und b; `between(None, Some(x)) < x`; 1 000 Mal am Anfang einfügen
   bleibt sortiert; Key-Länge wächst moderat (Assertion auf Maximallänge nach 1 000 Einfügungen).
4. **Property-Test** mit `proptest` (dev-dependency): zufällige Einfügepositionen → Liste ist immer streng sortiert.
**Fertig wenn:** Unit- und Property-Tests grün.
> Notiz: Eigene Implementierung (~200 Zeilen) nach Greenspan („Implementing Fractional Indexing"), kein Crate: Ganzzahlteil
> variabler Länge + Bruchteil → 1 000× am Anfang/Ende einfügen ergibt max. 3 Zeichen. `between` liefert `CoreResult`
> (Fehler bei `a >= b`; gleiche Keys sind nach parallelem Einfügen auf zwei Geräten möglich → Tiebreak in P5 klären).
> Zusätzlich `parse()` mit Formatprüfung. Randfall „Dekrement erreicht nackten kleinsten Ganzzahlteil" korrigiert.
> Für SQLite (P4): Spalte `sort_key` mit `BINARY`-Collation sortieren.

### [x] P1-07 · `TaskList`-Entity — 2026-10-17 · 92a9a24
**Ziel:** Listen („Privat", „Arbeit") plus feste Inbox.
**Schritte:**
1. `TaskList { id, name, sort_key, created_at, updated_at, is_inbox }`; Name-Validierung analog Titel (≤ 100).
2. Regel: genau eine Inbox; Inbox kann nicht gelöscht/umbenannt werden (Prüfung im Use-Case, hier nur Flag + Methode `can_delete()`).
3. Tests.
**Fertig wenn:** Tests grün; `cargo doc -p todo-core` ohne Warnungen.
> Notiz: Zusätzlich `can_rename()` (gleiche Regel wie `can_delete()`, für den Use-Case) und Konstante `INBOX_NAME` (UI übersetzt).
> `just doc` (rustdoc mit `-D warnings`) ist jetzt Teil von `just check`.

### [x] P1-08 · Review-Punkt P1 — 2026-10-17 · 7145a13
**Ziel:** Domain-Code vor dem Weiterbauen prüfen.
**Schritte:**
1. Abhängigkeiten von `todo-core` prüfen: nur `serde`, `uuid`, `thiserror`, Datums-Crate (+ dev: `proptest`).
2. `interfaces.md` §1 an tatsächlichen Code anpassen, falls abgewichen.
3. Testabdeckung grob prüfen (optional `cargo llvm-cov`).
**Fertig wenn:** Doku und Code stimmen überein; Commit `docs: …` falls nötig (nur getrackte Dateien).
> Notiz: Deps `todo-core`: chrono, serde, thiserror, uuid; dev: proptest, serde_json – wie vorgesehen (`serde_json` nur dev).
> Befund: Core baute nicht für wasm32 (uuid ohne Zufallsquelle) → `uuid/js` in `todo-wasm` + `just wasm` in `just check` (be41dc1).
> Proton-Check: nur Doku-Treffer. Abdeckung (`cargo llvm-cov`): 99,8 % Zeilen; Lücken nur `unreachable!` und unteres
> Wertebereichsende in `sort_key.rs`. `interfaces.md` §1, `module-structure.md` aktualisiert; ADR-010…012 ergänzt.

---

## P2 · Proton-Machbarkeit (Spike)

Lies vorher: `architecture.md` §7 (Risiken R-1–R-3), `tech-decisions.md` (Spikes S-1, S-2), `interfaces.md` §3–4.
**Spike-Code ist Wegwerf-Code** und lebt in `spikes/proton-probe/` (eigenes Crate, **nicht** im Workspace,
`spikes/` in `.gitignore` oder bewusst getrackt – Entscheidung in P2-01). Ziel ist Wissen, nicht schöner Code.

**Sicherheitsregeln für P2:** Ein **separates Test-Proton-Konto** verwenden. Zugangsdaten nur über Umgebungsvariablen
(`PROBE_USER`, `PROBE_PASS`, `PROBE_TOTP_SECRET`) oder interaktive Eingabe – nie in Dateien, Logs, Commits.
Tokens nach dem Spike widerrufen (Logout bzw. Sitzungen im Proton-Konto beenden).

### [x] P2-01 · Recherche: Wege zur Proton-API — 2026-10-17 · 4e0e6ba
**Ziel:** Entscheidungsgrundlage, bevor Code geschrieben wird.
**Schritte:**
1. Recherchieren und in `development/spikes/proton-research.md` festhalten (je mit Link, Lizenz, Sprache, Aktivität, Stand):
   - Offizielles Proton Drive SDK (Sprachen, Lizenz, ob Third-Party-Nutzung vorgesehen ist).
   - Proton-eigene Bibliotheken: `go-proton-api`, `go-srp`, `gopenpgp`, ggf. Rust-Pendants.
   - Community: rclone-Backend `protondrive` (+ zugrunde liegende Bridge-Bibliothek), Rust-Crates für Proton-API.
   - Protons Nutzungsbedingungen / Stellungnahmen zu Third-Party-Clients.
2. Auth-Ablauf dokumentieren: Endpunkte für SRP-Info/Auth, 2FA, Refresh, Header (`x-pm-appversion` – welche App-Version
   darf ein Drittclient senden?), Human-Verification-Verhalten.
3. Drive-Modell dokumentieren: Volume → Share → Link (Ordner/Datei) → Revision → Blöcke; welche Schlüssel wofür.
4. CORS-Frage für Web klären (Preflight-Test gegen API-Host mit fremder Origin, nur `OPTIONS`).
**Fertig wenn:** Recherche-Dokument existiert mit Empfehlung „Weg A/B/C" und offenen Fragen; User hat die Empfehlung gesehen.
> Notiz: Ergebnis: `development/spikes/proton-research.md` – Empfehlung Weg A (Rust-Eigenbau), Login per Browser-Session-Fork;
> User hat zugestimmt (2026-10-17). `spikes/` ist gitignored. CORS-Preflight noch offen: von der Sitzungs-Sicherheitsprüfung
> blockiert, User kann ihn selbst ausführen; Entscheidung spätestens P2-06 (Web nicht im Prototyp).
> Abweichung für P2-02/03: Fork statt SRP; SRP/2FA/HV nur dokumentiert (User-Entscheidung). Spike-ADR wird ADR-013.

### [x] P2-02 · Probe: Login (SRP) ohne 2FA — 2026-10-17 · 4e0e6ba
**Ziel:** Beweisen, dass ein eigener Client sich anmelden kann.
**Schritte:**
1. `spikes/proton-probe/` anlegen (Rust-Binary; falls P2-01 klar zu Go/TS rät, dort – Begründung notieren).
2. Login-Flow implementieren: Auth-Info holen → SRP-Proof berechnen (bestehende SRP-Bibliothek, keine Eigenbau-Krypto)
   → Session-Tokens erhalten.
3. Ausgabe: nur „Login OK, Scopes: …" – keine Tokens ausgeben.
**Fertig wenn:** Login gegen das Testkonto funktioniert reproduzierbar; Erkenntnisse im Recherche-Dokument ergänzt.
> Notiz: Umgesetzt als Browser-Session-Fork statt SRP (User-Entscheidung in P2-01). Probe `spikes/proton-probe` (gitignored,
> daher kein eigener Commit; Hash = Stand des Repos). Live-Login laut User erfolgreich; Detailausgabe nicht übermittelt.

### [x] P2-03 · Probe: 2FA, Refresh, Logout — 2026-10-17 · 41f151c
> Notiz (2026-10-17): P2-03…P2-05 wurden nach P3 mit den Live-Läufen des Users abgeschlossen.
**Ziel:** Alle Login-Stufen, die `AuthStep` abdecken muss, einmal real gesehen haben.
**Schritte:**
1. TOTP im Testkonto aktivieren; 2FA-Schritt implementieren.
2. Token-Refresh testen (Access-Token verfallen lassen oder Refresh direkt auslösen).
3. Logout/Session-Revoke.
4. Beobachten und notieren: Wann kommt Human Verification? Was passiert bei falschem Passwort, Rate-Limit (Fehlercodes)?
5. Two-Password-Mode nur dokumentieren (testen, falls mit vertretbarem Aufwand möglich).
**Fertig wenn:** Fehlercodes/Abläufe dokumentiert; `AuthStep`/`AuthError` in `interfaces.md` ggf. angepasst.
> Notiz: Live-Lauf `session` (Ausgabe gesehen): Refresh rotiert Token + entwertet alten Access-Token sofort; Logout +
> Refresh danach → 422/10013; App-Version-Fehler 5002/2064; ungültiger Token 401. 2FA/Human Verification/Two-Password
> erledigt der Browser-Fork (User-Entscheidung P2-01) → nur dokumentiert. Rate-Limits bewusst nicht provoziert.
> `interfaces.md` §3 (`AuthError`, Refresh-Regel) angepasst. Details: `spikes/proton-research.md` §13.

### [x] P2-04 · Probe: Drive lesen — 2026-10-17 · 41f151c
**Ziel:** Verschlüsselte Drive-Struktur entschlüsseln können.
**Schritte:**
1. Schlüssel-Kette aufbauen: User-Key → Adress-Key → Share-Key → Node-Keys (gemäß Recherche).
2. Root-Ordner des Haupt-Volumes listen, Ordner- und Dateinamen entschlüsselt ausgeben.
3. Eine kleine, manuell über die Proton-Weboberfläche hochgeladene Textdatei herunterladen und entschlüsseln.
**Fertig wenn:** Inhalt der Testdatei wird korrekt ausgegeben.
> Notiz: Lauf laut User problemlos (Ausgabe nicht übermittelt); Schlüsselkette bis Root-Ordner zusätzlich im
> `drive-write`-Lauf mit gesehener Ausgabe belegt. NodeHashKey ist 44-Byte-Base64-Text.

### [x] P2-05 · Probe: Drive schreiben — 2026-10-17 · 41f151c
**Ziel:** Alles, was `RemoteStore` braucht, einmal real durchgespielt.
**Schritte:**
1. Ordner `TasksProbe/` anlegen (falls vorhanden: wiederverwenden).
2. Neue Datei `hello-<timestamp>.bin` hochladen (Verschlüsselung, Block-Upload, Revision committen).
3. Datei in der Proton-Weboberfläche öffnen → Inhalt korrekt?
4. Datei löschen; Verhalten bei „Datei existiert schon" testen (Fehlercode notieren – wichtig für `create` ohne Überschreiben).
5. Events-/Änderungs-Endpunkt testen, falls vorhanden (für `changes_since`).
6. Latenzen messen (Listing, Upload 1 KB, Download 1 KB) und notieren.
**Fertig wenn:** Alle Operationen funktionieren; Latenzen und Fehlercodes dokumentiert.
> Notiz: Live-Lauf mit gesehener Ausgabe. Erster Lauf scheiterte am Block-Upload (400/2000 „outdated version") →
> Block-Verifikation nachgerüstet (siehe ADR-013-Nachtrag). Danach: Upload 1 KB 1 679 ms, Download 937 ms, Listing
> 242 ms; Duplikat 422/2500; Events (v1) liefern vollständige Links. Endgültiges Löschen über den Ordner-Endpunkt
> geht nur für Entwürfe (2501) → Papierkorb-Endpunkt in P6. Details: `spikes/proton-research.md` §13.

### [x] P2-06 · Spike-Abschluss & Go/No-Go — 2026-10-17 · 4e0e6ba
**Ziel:** Entscheidung, wie (und ob) Proton angebunden wird.
**Schritte:**
1. ADR-010 in `tech-decisions.md`: gewählter Implementierungsweg für `proton-adapter` (Rust-Eigenbau, Bibliothek X,
   Sidecar-Prozess …), Begründung, Risiken, Lizenzfolgen.
2. `interfaces.md` §3–4 auf Basis der Realität korrigieren (insb. Fehlerfälle, Revisionen, Events).
3. Web-Sync-Frage (CORS) entscheiden oder explizit vertagen.
4. Bei **No-Go**: User informieren; Alternativen aufzeigen (z. B. anderes `RemoteStore`-Backend wie WebDAV) –
   P3–P5 bleiben unverändert gültig, da sie Proton-unabhängig sind.
**Fertig wenn:** ADR-010 steht und der User hat zugestimmt.
> Notiz: ADR heißt ADR-013 (010–012 waren durch P1 belegt); vom User freigegeben. `interfaces.md` §3 auf Fork-Login
> (`OpenBrowser`/`Pending`/`Done`) umgestellt, §4 um Proton-Abbildung und Events-Pflicht ergänzt. CORS/Web-Sync vertagt
> (Web nicht im Prototyp). Abgeschlossen VOR P2-03…P2-05 auf Wunsch des Users; deren Live-Ergebnisse können den ADR
> noch präzisieren, nicht kippen.

---

## P3 · Ports, Fakes und Use-Cases (`crates/todo-core`)

Lies vorher: `interfaces.md` §0, §2, §6–7; `architecture.md` §3.

### [x] P3-01 · Async-Port-Infrastruktur — 2026-10-17 · 33ce184
**Ziel:** Ports funktionieren nativ (Send) und auf wasm32 (?Send).
**Schritte:**
1. `async-trait` hinzufügen; `MaybeSend`/`MaybeSync`-Marker-Traits; Attribut-Muster aus `interfaces.md` §0.
2. Optional: kleines `#[port]`-Makro als `macro_rules!` – nur wenn es lesbarer wird.
3. Test-Executor: `futures::executor::block_on` als dev-dependency (kein tokio im Core).
4. Kompilierprüfung: `cargo check -p todo-core --target wasm32-unknown-unknown`.
**Fertig wenn:** Nativ und wasm32 kompilieren; ein Dummy-Port mit Testimplementierung läuft im Test.
> Notiz: Kein `#[port]`-Makro – ein `macro_rules!`-Wrapper um den Trait verhindert rustfmt; Attributpaar steht explizit
> (Muster als Doctest in `ports/mod.rs`). `cargo check -p todo-core --target wasm32` braucht `--features uuid/js`
> (Zufallsquelle, siehe ADR-012); `just wasm` prüft so auch die Tests. `proptest` nur als native Dev-Dependency.
> `Clock` jetzt `MaybeSend + MaybeSync` (wie `interfaces.md` §6).

### [x] P3-02 · `ChangeSet`, `Op`-Platzhalter und `TaskRepository`-Port — 2026-10-17 · f9221a7
**Ziel:** Repository-Schnittstelle gemäß `interfaces.md` §2.
**Schritte:**
1. `ChangeSet` (Liste von Domain-Änderungen: `Upsert(Task)`, `UpsertList(TaskList)`, `Delete(EntityRef)`) definieren.
2. Sync-bezogene Methoden (`pending_ops`, `apply_remote` …) **noch nicht** – kommen in P5, damit P3 schlank bleibt.
   Notiz in der Datei hinterlassen.
3. `TaskQuery`, `SmartView`, `PageRequest`, `Page<T>`, `TaskSummary` definieren.
**Fertig wenn:** Kompiliert; Doku-Kommentare an jeder Port-Methode.
> Notiz: Abweichungen dokumentiert in `interfaces.md` §2: `SmartView::List(id)` statt `list`-Feld, `due_before` entfällt,
> Today/Upcoming tragen einen `LocalDay` (neu in `domain/due.rs`, mit `Due::is_due_by`), `search` entfällt im Prototyp,
> `get_task_list` neu. Semantik zentral in `TaskQuery::matches`. Kein `Op`-Platzhaltertyp – Ops kommen komplett in P5.

### [x] P3-03 · `InMemoryTaskRepository` (Test-Support) — 2026-10-17 · 523a86b
**Ziel:** Schnelle Fake-Implementierung für alle folgenden Tests.
**Schritte:**
1. Hinter Feature `test-support` (damit andere Crates sie in Tests nutzen können).
2. `BTreeMap`-basiert; Pagination per Cursor (`sort_key` + `id`); Filter für alle `SmartView`s.
3. Tests direkt gegen den Fake (Pagination-Grenzfälle: leere Liste, genau `limit`, `limit+1`).
**Fertig wenn:** Tests grün.
> Notiz: Liegt in `todo_core::testing` (dort kommen auch die Contract-Tests hin). Cursor-Format `s:<sort_key>:<id>` bzw.
> `c:<completed_at>:<id>` (opak für Aufrufer); Cursor einer anderen Sortierart → `Validation`. Löschen einer Liste mit
> Tasks wird durch die Integritätsregel abgelehnt (Use-Case muss Tasks vorher verschieben).

### [x] P3-04 · Wiederverwendbare Repository-Contract-Tests — 2026-10-17 · f0d1190
**Ziel:** Dieselbe Testsuite später gegen SQLite laufen lassen.
**Schritte:**
1. Modul `todo_core::testing::repository_contract` mit Funktion `run_all<R: TaskRepository>(make: impl Fn() -> R)`
   oder einem Makro, das je Fall einen `#[test]` erzeugt.
2. Fälle: Upsert+Get, Delete, Pagination, Filter Today/Upcoming/Completed, Sortierung nach `sort_key`.
3. Auf `InMemoryTaskRepository` anwenden.
**Fertig wenn:** Contract-Suite läuft grün gegen den Fake.
> Notiz: Beides umgesetzt – `run_all(make)` und Makro `todo_core::repository_contract_tests!(expr)` (ein `#[test]` je Fall,
> 20 Fälle). `futures` (nur executor) ist optionale Dependency hinter `test-support`. Zusätzlich `Task::move_to`
> (für „Liste löschen nach Verschieben" und P3-06). Mutationsprobe: ohne Integritätsprüfung bzw. ohne ID-Tiebreak
> schlagen 3 Fälle fehl.

### [x] P3-05 · Use-Cases: Listen — 2026-10-17 · 2a5b79a
**Ziel:** `EnsureInbox`, `CreateTaskList`, `RenameTaskList`, `DeleteTaskList` (Tasks wandern in Inbox).
**Schritte:**
1. Je Use-Case eine Datei in `app/`; Struktur: Input-Struct → `execute(&self, input) -> CoreResult<Output>`.
2. Tests mit `InMemoryTaskRepository` + `SteppingClock`.
**Fertig wenn:** Tests grün, inkl. „Inbox nicht löschbar".
> Notiz: Use-Cases halten `Deps { repo: Arc<dyn TaskRepository>, clock: Arc<dyn Clock> }`. `DeleteTaskList` lädt jede Task
> einzeln (`get_task`) – bei lokalem Speicher unkritisch, ggf. in P4 per Batch-Methode optimieren. Inbox-Regeln liefern
> `Validation`; unbekannte Listen `NotFound`. Test stellt sicher, dass Use-Case-Futures nativ `Send` sind.

### [x] P3-06 · Use-Cases: Tasks — 2026-10-17 · 14a4a10
**Ziel:** `CreateTask`, `UpdateTask` (Patch: Titel, Notizen, Priorität, Due), `CompleteTask`/`ReopenTask`,
`MoveTask` (Position + andere Liste), `DeleteTask`.
**Schritte:**
1. `MoveTask` nutzt `SortKey::between` mit Nachbarn (`before_id`/`after_id` als Input).
2. `CreateTask` ohne Liste → Inbox; ohne Position → ans Ende.
3. Tests je Use-Case + Fehlerfälle (`NotFound`, `Validation`).
**Fertig wenn:** Tests grün.
> Notiz: `MoveTaskInput { id, list_id, after, before }` – nur `after`/nur `before` bestimmen den fehlenden Nachbarn selbst;
> beide = dazwischen; keins = ans Ende. `CreateTask`/`MoveTask` laden dazu die Zielliste komplett (O(n)); falls das in
> P4-04 messbar wird: Repository-Methode für Nachbar-/letzten Sort-Key ergänzen. Gleiche Sort-Keys zweier Nachbarn
> (nach Sync) → `Validation` aus `SortKey::between` – Tiebreak siehe FIX-02.

### [x] P3-07 · Queries & Fassade `CoreApi` — 2026-10-17 · 41f151c
**Ziel:** Eine FFI-freundliche Fassade, die die UI-Shells später direkt verwenden.
**Schritte:**
1. DTOs (`TaskDto`, `TaskSummaryDto`, `TaskListDto`, Inputs) nur mit owned Typen, IDs als `String`.
2. `CoreApi` bündelt Use-Cases + Queries; nimmt `Arc<dyn TaskRepository>` + `Arc<dyn Clock>`.
3. Change-Benachrichtigung: `EventSink`-Trait (`fn emit(&self, event: CoreEvent)`), `CoreEvent::Changed { ids }`.
4. Integrationstest in `tests/api.rs`: kompletter Ablauf (Inbox → Task anlegen → verschieben → abhaken → listen).
**Fertig wenn:** Integrationstest grün; `interfaces.md` §7 aktualisiert.
> Notiz: `CoreEvent::Changed { tasks, lists }` statt `{ ids }` (UI unterscheidet Sidebar/Liste); Events kommen aus einem
> Repository-Decorator, nicht aus den Use-Cases. DTO-Zeitpunkte als UTC-ISO-8601 (chrono-Feature `alloc`).
> `tests/api.rs` braucht `test-support` → `required-features`; `just test` läuft deshalb mit `--all-features`.

---

## P4 · SQLite-Speicher (`crates/todo-storage-sqlite`)

Lies vorher: `architecture.md` §4 (Linux), §6 (Daten-Fallstricke).

### [x] P4-01 · Verbindung & Konfiguration — 2026-10-17 · 096b452
**Ziel:** Korrekt konfigurierte SQLite-Verbindung.
**Schritte:**
1. `rusqlite` mit Feature `bundled`.
2. `open(path)` setzt: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout`.
3. `open_in_memory()` für Tests.
4. Async-Strategie entscheiden: Verbindung in einem dedizierten Thread mit Kanal **oder** `Mutex<Connection>` +
   Aufruf aus `spawn_blocking` der Shell. Entscheidung als Notiz festhalten (Core bleibt runtime-frei).
**Fertig wenn:** Test prüft, dass die Pragmas gesetzt sind.
> Notiz: Async-Strategie = **dedizierter Thread** besitzt die `Connection`; Aufrufer schicken Closures per `mpsc`,
> Antwort per `futures::channel::oneshot` (runtime-frei, kein Lock über `.await`, kein `spawn_blocking` in der Shell
> nötig) → ADR-014. Zusätzlich `busy_timeout` = 5 s, Statement-Cache 64. WAL wird für Dateien erzwungen (Fehler, falls
> das Dateisystem es nicht kann); In-Memory läuft ohne WAL. Panic im DB-Thread → danach `Storage`-Fehler.

### [x] P4-02 · Migrationen — 2026-10-17 · 9311760
**Ziel:** Versioniertes Schema.
**Schritte:**
1. `migrations/0001_init.sql`: `task_lists`, `tasks` (+ Indizes `(list_id, sort_key)`, `(status, due_date)`), `meta`.
2. Kleiner Migrations-Runner über `PRAGMA user_version` (eigene ~40 Zeilen statt schwerer Crate), SQL via `include_str!`.
3. Tests: frische DB → aktuelle Version; zweimal migrieren ist idempotent.
**Fertig wenn:** Tests grün.
> Notiz: Tabellen `STRICT`; `tasks.list_id` als **aufgeschobener** Fremdschlüssel (`DEFERRABLE INITIALLY DEFERRED`) –
> Integrität wird beim COMMIT geprüft, Verstoß → ganze Transaktion scheitert (erfüllt „Task verweist auf existierende
> Liste" ohne O(n)-Prüfung). CHECKs für `status`/`completed_at` und `due_*`. `due_date` als Tage seit 0001-01-01
> (INTEGER, sortierbar für alle `NaiveDate`), `due_at`+`due_tz` für Zeitpunkte. Tags vorerst als leerzeichengetrennte
> ID-Liste in `tasks.tags` (eigene Tabelle, sobald Tags Logik bekommen). Zusätzliche Indizes `(sort_key, id)`,
> `(status, due_at)`, `(status, completed_at)`. Runner: eine Transaktion je Migration, neuere DB-Version → Fehler.

### [x] P4-03 · `SqliteTaskRepository` — 2026-10-17 · f5d434d
**Ziel:** Vollständige Port-Implementierung.
**Schritte:**
1. Mapping Row ↔ Domain in eigenem Modul; `commit(ChangeSet)` in **einer** Transaktion.
2. Prepared Statements cachen (`prepare_cached`).
3. Contract-Suite aus P3-04 gegen SQLite laufen lassen (In-Memory-DB + Temp-Datei).
**Fertig wenn:** Contract-Suite grün gegen SQLite.
> Notiz: Core um `TaskParts`/`TaskListParts` + `from_parts` erweitert (Wiederherstellen mit Invariantenprüfung;
> verletzte Invarianten in der DB → `Storage("corrupt …")`). Contract-Suite läuft zweimal (In-Memory + Temp-Datei über
> Delegations-Wrapper, der das Temp-Verzeichnis am Leben hält). Paginierung per Keyset (`(sort_key, id) > (?, ?)`,
> Erledigt: `completed_at DESC, id`), Cursor-Format wie im Fake. Tag-Filter per `instr(tags, ?)`. Zusätzlich
> `tests/api.rs`: `CoreApi` auf SQLite inkl. Neustart.

### [x] P4-04 · Performance-Test Speicher — 2026-10-17 · 32757e4
**Ziel:** Frühwarnung bei langsamen Queries.
**Schritte:**
1. Test (ignored, manuell via `cargo test -- --ignored` oder als `examples/bench_storage.rs`): 10 000 Tasks einfügen,
   erste Seite (50) der Inbox laden, Task verschieben.
2. Ziele: Seite laden < 5 ms, einzelner Commit < 2 ms (Release-Build). `EXPLAIN QUERY PLAN` prüfen: Index wird genutzt.
**Fertig wenn:** Messwerte in `architecture.md` §6 notiert; Ziele erreicht oder Abweichung erklärt.
> Notiz: Ziele deutlich erreicht (Seite ~0,06 ms, Commit ~0,09 ms p50 auf btrfs) – Werte in `architecture.md` §6.
> `EXPLAIN QUERY PLAN` deckte Sortierschritte bei Inbox (`IN`-Unterabfrage), Alle+Status und Erledigt auf → behoben
> (Inbox-ID vorab auflösen; Indizes `(status, sort_key, id)`, `(status, completed_at DESC, id)` direkt in `0001`, da
> noch nie ausgeliefert). Plan-Regressionstest im Crate. Use-Case-Ebene `create_task`/`move_task` ~7 ms bei 10 000
> Tasks in einer Liste (O(n) aus P3-06) → FIX-03.

### [x] P4-05 · Dev-CLI (optional, empfohlen) — 2026-10-17 · 0756a5a
**Ziel:** Core + SQLite ohne UI benutzen und debuggen.
**Schritte:**
1. `crates/todo-cli` (Binary, nicht Teil der App-Auslieferung): `add`, `ls`, `done`, `mv`, `rm` gegen eine DB-Datei.
2. Nutzt ausschließlich `CoreApi`.
**Fertig wenn:** Manuell: Tasks anlegen, listen, abhaken funktioniert; DB bleibt nach Neustart erhalten.
> Notiz: Befehle `lists`, `list-add`, `list-rm`, `add`, `ls [inbox|all|done|today|upcoming|LISTE] [--open]`, `show`,
> `done`/`undo`, `edit`, `mv`, `rm`; IDs als eindeutiges Ende (Kurz-ID aus `ls`). DB: `--db` > `$TODO_DB` >
> `./todo-dev.sqlite3` (gitignored). Argument-Parsing ohne Crate. Today/Upcoming mit UTC-Tagesgrenzen (CLI hat keine
> Zeitzonenlogik). Manuell durchgespielt (jeder Befehl eigener Prozess → Persistenz bestätigt) und zusätzlich als
> Binary-Test `tests/cli.rs` automatisiert. Start ruft `ensure_inbox` wie später die App.

---

## P5 · Sync-Engine gegen Fakes (`crates/todo-sync`)

Lies vorher: `sync-strategy.md` vollständig.

### [x] P5-01 · Hybrid Logical Clock — 2026-10-17 · 328359b
**Ziel:** Kausal korrekte, totale Ordnung von Ops.
**Schritte:**
1. `Hlc { wall_ms, counter, device: DeviceId }`, `Ord` (wall, counter, device).
2. `HlcState::tick(now)` und `HlcState::observe(remote, now)` nach Standard-Algorithmus; Schutz gegen Remote-Zeit
   > jetzt + 24 h (siehe `sync-strategy.md` §5).
3. Kompakte String-/Byte-Serialisierung (sortierbar).
4. Tests + Property-Test: `tick` ist streng monoton, auch wenn die Uhr rückwärts springt.
**Fertig wenn:** Tests grün.
> Liegt in `todo_core::sync::hlc` statt `todo-sync` (ADR-015: Op-Typen brauchen die HLC, der Repository-Port die Ops).
> `wall_ms: u64`, `counter: u32` (Überlauf rückt `wall_ms` vor), Bytes = 28 (BE, memcmp-sortierbar), String = 56 Hex-Zeichen;
> serde: JSON als String, Binärformate als Bytes. `observe` übernimmt fremde Werte > jetzt + 24 h nicht (`ClockSkew`).

### [x] P5-02 · Op-Modell & Serialisierung — 2026-10-17 · 29da98b
**Ziel:** `Op`, `OpKind`, `FieldName`, `FieldValue` gemäß `sync-strategy.md` §2.
**Schritte:**
1. Format wählen: `postcard` oder CBOR (`ciborium`) – kleiner und schneller als JSON; Entscheidung notieren.
2. `FieldName` als Enum (nicht freier String) mit stabilen numerischen Codes (Formatstabilität!).
3. Golden-File-Test: festes Op-Beispiel → Bytes in `tests/golden/` → muss byte-gleich bleiben.
**Fertig wenn:** Roundtrip- und Golden-Tests grün.
> Format: **postcard** (ADR-016), Abhängigkeit von `todo-core`; Typen in `todo_core::sync::op` (ADR-015).
> `FieldName` mit festen `u8`-Codes; `FieldValue` typisiert (`Text`, `Status(Option<completed_at>)`, `Due`, …), Prüfung per
> `FieldName::accepts`/`applies_to` beim Dekodieren. IDs/HLC binär roh (16/28 Bytes), in JSON weiterhin Strings.
> Golden File: `crates/todo-core/tests/golden/ops_v1.bin` (Test `tests/op_format.rs`, Neuerzeugung nur mit `UPDATE_GOLDEN=1`).

### [x] P5-03 · Diff: Domain-Änderung → Ops — 2026-10-17 · 24d821f
**Ziel:** Use-Cases erzeugen automatisch passende Ops.
**Schritte:**
1. Funktion `ops_for(change: &Change, before: Option<&Task>, hlc: &mut HlcState) -> Vec<Op>`:
   Create → `Create{fields}`, geänderte Felder → je ein `Set`, Delete → `Delete`.
2. Tests: Titeländerung erzeugt genau einen `Set`-Op; unveränderte Felder erzeugen nichts.
**Fertig wenn:** Tests grün.
> Signatur: `ops_for(change, before: Option<&FieldMap>, clock: &mut OpClock, now)` – `before` sind die bekannten
> Feldwerte (Register) statt `&Task`, damit Listen und teilweise bekannte Entities gleich behandelt werden.
> `OpClock` = `HlcState` + lückenlose Op-Nummer. `task_fields`/`list_fields` bilden Entities auf Felder ab; `updated_at`
> erzeugt keine Ops. Löschen unbekannter Entities erzeugt nichts.

### [x] P5-04 · Merge (LWW + Tombstones) — 2026-10-17 · f27312a
**Ziel:** Deterministisches Zusammenführen.
**Schritte:**
1. Reine Funktion `merge(state: &EntityState, field_clocks: &FieldClocks, ops: &[Op]) -> MergeResult`.
2. Regeln aus `sync-strategy.md` §2 (feldweise LWW, Delete-wins, Tie-Break `device_id`, Duplikate idempotent).
3. Unit-Tests für jede Regel mit sprechenden Namen (`concurrent_title_and_due_edits_both_survive` …).
4. **Property-Test Konvergenz:** 2–4 simulierte Geräte, zufällige Ops, zufällige Lieferreihenfolge + Duplikate →
   identische Endzustände.
**Fertig wenn:** Unit- und Property-Tests grün (Property-Test mit ≥ 1 000 Fällen lokal).
> In `todo_core::sync::merge` (ADR-015). Signatur `merge(state: Option<&EntityState>, ops) -> MergeResult { state, changed }`:
> `EntityState::Live(Registers)` (Register = Wert + HLC je Feld, zugleich die Field-Clocks) oder `Deleted(hlc)`.
> Zusätzlich `materialize_task`/`materialize_list` (nur bei vollständigen, gültigen Registern; `updated_at` = größte HLC-Zeit).
> Property-Test `replicas_converge` (1 000 Fälle, 2–4 Geräte, springende Uhren, Mischen/Gruppieren/Duplikate);
> per Mutation geprüft, dass er falsches LWW erkennt.

### [x] P5-05 · Repository-Erweiterung für Sync — 2026-10-17 · d4939eb
**Ziel:** Outbox, Field-Clocks, Cursor persistieren.
**Schritte:**
1. Port erweitern: `pending_ops`, `mark_ops_pushed`, `apply_remote`, `sync_cursor`, `set_sync_cursor`, `field_clocks`.
2. `commit` schreibt nun Zustand + Ops + Field-Clocks atomar. (Op-Erzeugung findet in der Anwendungsschicht statt –
   Abhängigkeitsrichtung prüfen: Op-Typen ggf. in `todo-core` verschieben, Merge-Logik bleibt in `todo-sync`.)
3. Migration `0002_sync.sql` (`outbox_ops`, `field_clocks`, `remote_cursors`, `sync_meta`).
4. In-Memory- und SQLite-Implementierung + Contract-Tests erweitern.
**Fertig wenn:** Contract-Suite grün für beide Implementierungen; Absturz-Test: Commit abgebrochen → weder Zustand noch Op geschrieben.
> Abweichungen (ADR-015): Op-Erzeugung und Merge laufen **im Repository-Commit** über `todo_core::sync::record_local`/
> `apply_remote` (generisch über `ReplicaStore`), nicht in der Engine – keine Races zwischen Lesen und Schreiben.
> `field_clocks` speichert Wert **und** HLC je Feld (LWW-Register); Lesezugriff heißt `entity_state`. Zusätzlich:
> `device_id`, `pending_op_count`, `sync_value`/`set_sync_value`, Tabelle `tombstones`. Zeit für die HLC über `ChangeSet::at`
> (setzt der `CoreApi`-Decorator); `CoreApi::repository()` für die Engine (Events auch bei Remote-Änderungen).
> Inbox hat feste ID `TaskListId::INBOX`. Tasks ohne existierende Liste landen beim Materialisieren in der Inbox.
> SQLite: `0002_sync.sql`; alte Daten (Schema 1) werden beim ersten Öffnen als `Create`-Ops eingereiht.
> Absturz-Test: Trigger lässt den Outbox-Insert scheitern → weder Zustand noch Op noch Register. Benchmark unverändert (Commit ~0,07 ms).

### [x] P5-06 · `RemoteStore`-Port + Fakes — 2026-10-17 · ad01588
**Ziel:** Backend-Abstraktion gemäß `interfaces.md` §4 (in der Form nach P2-06).
**Schritte:**
1. Port in `todo-core::ports::remote_store`.
2. `InMemoryRemoteStore` mit Fehler-Injektion (Timeout, 429, Conflict, künstliche Latenz).
3. `FsRemoteStore` (lokales Verzeichnis; `create` mit `O_EXCL`-Semantik).
4. Contract-Tests für beide (insb. `create` überschreibt nie).
**Fertig wenn:** Tests grün.
> Port `todo_core::ports::RemoteStore` mit validiertem `RemotePath`; `list` ist rekursiv (alle Dateien unter dem Ordner, sortiert).
> `InMemoryRemoteStore` in `todo_core::testing`: Änderungs-Feed, Fehler-Injektion (`Timeout`, `RateLimited`, `Rejected`,
> `LostResponse` = ausgeführt, Antwort verloren), Latenz als Executor-Yields (keine echte Zeit), `tamper`, Aufrufzähler.
> `FsRemoteStore` im neuen Crate `todo-remote-fs` (`create` = Temp-Datei + `hard_link`, atomar, nie überschreibend; kein Feed).
> Gemeinsame Suite `remote_store_contract_tests!`. Neu: `CoreError::RateLimited { retry_after_s }` + `is_retryable()`.

### [x] P5-07 · `Cipher`, `KeyStore` & Segment-Format — 2026-10-17 · c261284
**Ziel:** Verschlüsselte, komprimierte, unveränderliche Segmente.
**Schritte:**
1. `Cipher`-Port + `XChaCha20Poly1305Cipher` (Crate `chacha20poly1305`), AAD gemäß `interfaces.md` §5.
2. `KeyStore`-Port + `InMemoryKeyStore`.
3. Segment: Header + zstd (`zstd` nativ; für wasm später `ruzstd` prüfen – Feature-Flag vorsehen) + Verschlüsselung.
4. Tests: Roundtrip; manipuliertes Byte → Fehler; falsches AAD (anderes Gerät/Seq) → Fehler; Golden-File für Header.
**Fertig wenn:** Tests grün.
> ADR-017. Ports `Cipher` (mit `SecretBytes` statt `KeyId`, plus `generate_key`) und `KeyStore` in `todo_core::ports`;
> `InMemoryKeyStore` in `todo_core::testing`. `XChaCha20Poly1305Cipher` im neuen Crate `todo-crypto` (`getrandom` 0.4 wie `uuid`;
> `todo-wasm` aktiviert `getrandom/wasm_js`). Segment: 40-Byte-Header = AAD; Body = seal(zstd(postcard(ops))).
> Features `zstd-native` (Standard) / `pure-rust-zstd` (`ruzstd`, von `todo-wasm` genutzt) – `just test` prüft beide.
> Golden File `crates/todo-sync/tests/golden/segment_header_v1.bin`. Tests: Roundtrip, jedes manipulierte Byte, fremder
> Header (anderes Gerät/Bereich = falsches AAD), falscher Schlüssel, neuere Formatversion.

### [x] P5-08 · Sync-Engine: Bootstrap, Push, Pull — 2026-10-17 · fa2b617
**Ziel:** Ein Sync-Durchlauf gemäß `sync-strategy.md` §4.
**Schritte:**
1. `SyncEngine::new(repo, remote, cipher, key_store, clock, device_id)`.
2. `bootstrap()`: App-Ordner, `manifest.json`, Sync-Schlüssel (erstes vs. weiteres Gerät), `devices/<id>.json`.
3. `push()`: Outbox in Segmente (≤ 500 Ops), `create`, `mark_ops_pushed`; Conflict-Fall idempotent.
4. `pull()`: fremde Segmente nach Cursor, entschlüsseln, mergen, `apply_remote`, Cursor setzen; beschädigte Segmente überspringen.
5. `sync_once() = push + pull`, liefert `SyncReport` (gepusht, empfangen, Fehler).
**Fertig wenn:** Integrationstest mit `InMemoryRemoteStore`: Gerät A erzeugt Tasks → Sync → Gerät B (frische DB) sieht sie.
> `SyncEngine::new(repo, remote, cipher, key_store, clock, config)` – Geräte-ID kommt aus `repo.device_id()` statt als Parameter;
> `repo` = `CoreApi::repository()` (Events bei Remote-Änderungen). `bootstrap()` → `Bootstrap::{Ready, NewerFormat}`; Schlüssel unter
> `sync-key:<Ordner>` im KeyStore, im Ordner als `keys/sync-key.enc` (`TKEY`+Version+Bytes). `pull` nutzt den Änderungs-Feed
> (nur geänderte Geräteordner listen), sonst Listing; Cursor nur über lückenlose Segmentketten; ≤ 4 parallele Downloads,
> ≤ 20 Segmente je `apply_remote`. `sync_once` lehnt parallele Läufe ab (`Conflict`). Neu im Port: `sync_cursors()`.
> Integrationstests `crates/todo-sync/tests/engine.rs` (14), u. a. A erzeugt → Sync → frisches B sieht alles (In-Memory und SQLite),
> verlorene Upload-Antwort, kaputtes Segment, neueres Layout, Feed ohne Listing, 1 100 Ops → 3 Segmente.

### [x] P5-09 · Mehrgeräte-Szenarien — 2026-10-17 · aabf718
**Ziel:** Echte Offline-Konflikte abgesichert.
**Schritte:**
Integrationstests (je ein Test) mit 2–3 Geräten über einen gemeinsamen `FsRemoteStore` (Temp-Dir):
1. Beide offline, A ändert Titel, B ändert Due → nach Sync beide Änderungen auf beiden Geräten.
2. A löscht, B bearbeitet dieselbe Task → gelöscht auf beiden.
3. Beide verschieben verschiedene Tasks in derselben Liste → Reihenfolge identisch auf beiden.
4. Push bricht mitten ab (Fehler-Injektion) → nächster Sync heilt, keine Duplikate.
5. Uhr von B geht 2 h vor → Ergebnis trotzdem konsistent.
**Fertig wenn:** Alle Szenarien grün.
> `crates/todo-sync/tests/scenarios.rs`: gemeinsames `FsRemoteStore`-Temp-Verzeichnis, je Gerät eine SQLite-Datei, Vergleich per
> `assert_converged` (Tasks inkl. Reihenfolge, Listen, Notizen/Liste/created_at). Szenario 3 erzwingt gleiche Sort-Keys
> (Tiebreak `TaskId`). Szenario 4: Test-Wrapper `Flaky` um den `FsRemoteStore` – Abbruch beim 2. von 3 Segmenten, danach
> verlorene Antwort nach Neustart des Prozesses; B sieht 1 200 Tasks genau einmal. Szenario 5 zusätzlich mit drittem Gerät.

### [x] P5-10 · Scheduler & `SyncStatus` — 2026-10-17 · 688ac76
**Ziel:** Wann wird gesynct – ohne Polling im Leerlauf.
**Schritte:**
1. Runtime-unabhängiger Zustandsautomat: Eingänge `LocalChange`, `AppFocused`, `AppHidden`, `NetworkUp/Down`,
   `TimerFired`, `SyncFinished(Result)`; Ausgabe: „jetzt syncen" / „Timer in X ms stellen" / `SyncStatus`.
2. Debounce 3 s, Backoff 5 s → 10 min mit Jitter, `Retry-After`, kein Timer wenn versteckt.
3. Reine Tests mit `SteppingClock` (keine echten Sleeps).
**Fertig wenn:** Tests grün; Shells müssen nur noch Timer + Events anschließen.
> `todo_sync::scheduler`: `Scheduler::handle(event, now) -> Actions { sync_now, timer_in_ms, status }` – die Shell stellt genau einen
> Timer (ersetzen/abbrechen) und meldet `SyncFinished(SyncOutcome { result, pending_ops })` zurück. Zusätzliche Eingänge:
> `SetEnabled`, `AppStarted`, `ManualSync`. Jitter über deterministischen xorshift-PRNG (Seed von der Shell), daher ohne Uhr/Zufall
> testbar. Periodischer Pull nur sichtbar (60 s → 10 min ohne Aktivität), per `poll_min_ms: None` abschaltbar.
> `SyncStatus` liegt in `todo-sync` (`Offline { pending_ops: u64 }`); DTO/Event für die UI folgt mit der Shell (P7).
> `AuthRequired` existiert als Pause-Grund, wird aber erst mit dem Auth-Port (P6) ausgelöst. 16 reine Tests (u. a. mit `SteppingClock`).

### [x] P5-11 · Snapshots & Kompaktierung (Minimalversion) — 2026-10-17 · f0b12ed
**Ziel:** Neues Gerät muss nicht alle Segmente lesen; Op-Log wächst nicht unbegrenzt.
**Schritte:**
1. Snapshot schreiben (Schwellen aus `sync-strategy.md` §4) und beim Bootstrap eines neuen Geräts nutzen.
2. Löschen eigener Segmente nur unter den dokumentierten Bedingungen.
3. Test: 3 000 Ops → Snapshot → neues Gerät lädt Snapshot + Rest, Zustand identisch.
**Fertig wenn:** Test grün. (Tombstone-Bereinigung und Full-Resync dürfen nach dem Prototyp kommen – als offene Aufgabe notieren.)
> `todo_sync::snapshot` (Header `TSNP` = AAD, Golden File `tests/golden/snapshot_header_v1.bin`). Die Engine schreibt bei
> mehr als 2 000 Ops bzw. 7 Tagen (konfigurierbar, nur mit leerer Outbox) und importiert per State-Join über `apply_remote`
> (neues Gerät; Lücke am Kettenanfang). Kompaktierung bewusst vorsichtiger als im Plan: nur was der **vorletzte** eigene
> Snapshot abdeckt und alle aktiven Geräte gelesen haben; das jüngste Segment bleibt immer (Lückenerkennung). Neu im Port:
> `entity_states()`. Tests `crates/todo-sync/tests/snapshots.rs` (5): 3 001 Ops → Snapshot → neues Gerät lädt Snapshot + nur
> das spätere Segment, Zustand identisch; Kompaktierung wartet auf aktive Geräte; abgehängtes Gerät holt per Snapshot auf;
> kaputter neuester Snapshot. Offen (unter „Nach dem Prototyp" notiert): Tombstone-Bereinigung, Full-Resync nach > 90 Tagen.

### [x] P5-12 · Dev-CLI mit Sync — 2026-10-17 · 0191422
**Ziel:** Sync manuell erlebbar, bevor Proton dran ist.
**Schritte:**
1. `todo-cli --db a.db --remote ./remote sync` (nutzt `FsRemoteStore`).
2. Manuell mit zwei DBs durchspielen: anlegen in A, sync A, sync B, `ls` in B.
**Fertig wenn:** Manueller Durchlauf funktioniert (kurz in der Aufgabe notieren).
> `todo-cli [--db DATEI] [--remote ORDNER] sync` (Ordner alternativ über `$TODO_REMOTE`), `FsRemoteStore` + XChaCha20; der
> Sync-Schlüssel wird pro Aufruf aus `keys/sync-key.enc` geladen (prozesslokaler KeyStore, nur Dev). Manuell durchgespielt
> (2026-10-17): A: `add Milch kaufen`, `list-add Arbeit`, `add Bericht schreiben -l Arbeit`, `sync` → „4 Op(s) hochgeladen";
> B (frische DB): `sync` → „1 hochgeladen (eigene Inbox), 4 empfangen"; `ls` zeigt „Milch kaufen", `ls Arbeit` „Bericht
> schreiben", `lists` genau eine Inbox; zweiter `sync` ist No-op. Zusätzlich automatisiert in `crates/todo-cli/tests/cli.rs`
> (inkl. Rückweg B → A).

---

## P6 · Proton-Adapter produktiv (`crates/proton-adapter`)

Lies vorher: ADR-010 und `development/spikes/proton-research.md` aus P2; `interfaces.md` §3–5.
Grundsatz: Erkenntnisse aus dem Spike **neu und sauber** umsetzen, nicht Spike-Code kopieren.

### [x] P6-01 · HTTP-Client-Grundgerüst — 2026-10-17 · ba37f04
**Ziel:** Robuster API-Client.
**Schritte:**
1. HTTP-Client mit `rustls` (kein OpenSSL – wichtig für Android später), gemeinsame Header, Timeouts.
2. Fehler-Mapping Proton-Code → `CoreError` (retrybar vs. endgültig), `Retry-After`-Auswertung.
3. Logging über `tracing` mit Redaction: Tokens, Passwörter, Dateinamen nie loggen.
4. Tests gegen lokalen Mock-Server (z. B. `wiremock`) mit Fixtures aus dem Spike (bereinigt, keine echten Daten).
**Fertig wenn:** Tests grün; Redaction-Test (Token taucht im Log nicht auf).
> Notiz: ADR-018. `reqwest` ohne Default-Features + `rustls`/`ring` (kein aws-lc), braucht tokio-Kontext. `api_client.rs`
> (Header, Timeouts 30 s/10 s, Storage-Up/Download), `error.rs` (`ApiError` → `CoreError`; Server-Fehlertexte werden nie
> übernommen), `secret.rs`. Kein Retry im Client (Scheduler macht Backoff). Logs nur mit Routen-Vorlagen – der Fork-Selector
> im Pfad ist ein Geheimnis. Redaction-Test über globalen Capture-Subscriber (scoped Subscriber + parallele Tests = Callsite-
> Cache-Problem). Fixtures `crates/proton-adapter/tests/fixtures/` (Werte erfunden). 401 → vorerst `RemoteRejected`,
> App-Version-Codes → `RemoteRejected`; Feinmapping (`AuthRequired`, `ClientRejected`) mit P6-03.

### [x] P6-02 · `SecretServiceKeyStore` (Linux) — 2026-10-17 · 2de7acc
**Ziel:** Tokens und Schlüssel sicher im Systemschlüsselbund.
**Schritte:**
1. Crate `keyring` oder `oo7` wählen (Größe/Abhängigkeiten vergleichen); in eigenem kleinem Crate oder Modul der Desktop-Shell
   (nicht im Proton-Adapter – KeyStore ist plattform-, nicht proton-spezifisch). Ort festlegen und `module-structure.md` aktualisieren.
2. Manueller Test mit GNOME Keyring/KWallet: put/get/delete; Verhalten ohne laufenden Secret Service dokumentieren.
**Fertig wenn:** Manueller Test erfolgreich; Fehlermeldung bei fehlendem Secret Service verständlich.
> Notiz: `oo7` 0.6 (tokio, `native_crypto`) statt `keyring` 4: rein Rust, async passend zum Port, Flatpak-Portal automatisch;
> 96 vs. 84 Crates, zbus/tokio teilt die Shell. Eigenes Crate `todo-keystore-secret-service` (nur `cfg(linux)`), `connect(app_id,
> app_label)`; Attribute `application`/`key`, Label `<app_label>: <name>`, gesperrte Collection/Items werden entsperrt.
> Manueller Test (`--test live -- --ignored`) gegen GNOME Keyring (KDE-Session, `org.freedesktop.secrets` = gnome-keyring)
> grün: put/get/überschreiben/fremde App-ID/delete idempotent. Ohne Bus bzw. mit leerem Bus (`dbus-run-session`): sofort
> `Storage("system keyring not reachable (… org.freedesktop.secrets): …")`, kein Hängen – automatisiert in `tests/no_service.rs`.

### [x] P6-03 · `ProtonAuthProvider` — 2026-10-17 · a207d8b
**Ziel:** Login-State-Machine hinter `AuthProvider`.
**Schritte:**
1. `begin_login`, `continue_login` (TOTP, zweites Passwort, Human-Verification-Token), `resume`, `logout`.
2. Refresh-Token + nötige Schlüssel-Materialien über `KeyStore`; Access-Token nur im Speicher.
3. Automatischer Refresh bei 401 im HTTP-Client (einmalig, dann `AuthRequired`).
4. Mock-Server-Tests für jeden `AuthStep`-Übergang; ein `#[ignore]`-Test gegen das echte Testkonto
   (nur mit gesetzten Env-Variablen).
**Fertig wenn:** Mock-Tests grün; ignorierter Live-Test einmal manuell erfolgreich.
> Notiz: Mock-Tests grün; Live-Test (2026-10-17, `development/live-p6.sh`) **OK**: Fork-Login nach 6 s (kein Pending,
> User war im Browser angemeldet), `resume` = Refresh mit Token-Rotation 775 ms, `logout` löscht lokal, alte (widerrufene)
> Sitzungsdaten → `SessionRevoked`.
> Port `ports/auth.rs` nach `interfaces.md` §3; `CoreError::{AuthRequired, Auth}`; Scheduler pausiert bei Auth-Fehlern.
> Abweichung: kein `continue_login` (TOTP/zweites Passwort/HV) – Fork-Login im Browser (ADR-013). `resume` offline-tolerant.

### [x] P6-04 · `ProtonDriveRemoteStore` — 2026-10-17 · c11bf03, e35862a
**Ziel:** `RemoteStore`-Port auf Proton Drive.
**Schritte:**
1. `open_app_root`: App-Ordner finden/anlegen; Pfad → Link-ID-Cache (im Speicher, invalidiert bei Fehlern).
2. `list`, `read`, `create` (ohne Überschreiben), `replace` (mit Revision), `delete`, `changes_since` (falls vorhanden).
3. Proton-Typen bleiben in `mapping.rs`; nach außen nur Port-Typen.
4. Die **`RemoteStore`-Contract-Tests aus P5-06** als `#[ignore]`-Live-Test gegen das Testkonto laufen lassen
   (eigener Testordner, wird am Ende aufgeräumt).
**Fertig wenn:** Live-Contract-Tests einmal manuell grün; Mock-Tests grün.
> Notiz: Mock-Tests inkl. kompletter Contract-Suite gegen zustandsbehafteten Fake-Server (`drive/fake.rs`) grün.
> Live-Contract (`development/live-p6.sh`, Unterordner je Fall, am Ende gelöscht): 1. Lauf 7/8 – eigene Löschungen fehlten
> im Feed (Lösch-Event trägt nur die LinkID, Zuordnung war schon verworfen; Papierkorb-Event live nicht ausreichend) →
> Fix e35862a, Fake bildet das jetzt nach. 2. Lauf **8/8 OK** (49 s gesamt; je Fall 1,9–9,7 s, Netz deutlich langsamer als
> im 1. Lauf mit 0,6–3,4 s). Ordnerstruktur in der Weboberfläche vom User geprüft.

### [x] P6-05 · Live-Sync über Proton (CLI) — 2026-10-17 · 39e7efa
**Ziel:** Sync-Engine + Proton zusammen, noch ohne UI.
**Schritte:**
1. `todo-cli login` (interaktiv, inkl. 2FA) und `todo-cli sync --remote proton`.
2. Durchlauf mit zwei DBs gegen das Testkonto; Ordnerstruktur in der Proton-Weboberfläche kontrollieren
   (Dateinamen sinnvoll, keine Klartext-Inhalte).
3. Latenz eines Sync-Durchlaufs messen und notieren.
4. Isolationsprüfung: `rg -il proton crates/todo-core crates/todo-sync` → keine Code-Treffer.
**Fertig wenn:** Zwei DBs synchronisieren über Proton Drive; Messwerte notiert.
> Notiz: `login`/`logout`/`sync --remote proton` (Sitzung + Sync-Schlüssel im Secret Service, App-ID `dev.tasks.todo-cli`).
> Live-Durchlauf (2026-10-17, Release-Build, zwei DBs, ein Rechner): A legt 2 Tasks + Liste an → B sieht beide (inkl. Liste,
> genau eine Inbox); B legt Task an → A sieht sie. Weboberfläche laut User „sieht alles gut aus" (Namen sinnvoll, Inhalte
> verschlüsselt). Messwerte je CLI-Aufruf (**kalt**: neuer Prozess = Refresh + Schlüsselkette + Ordnerauflösung jedes Mal):
> A erster Push (4 Ops, Ordner/Manifest/Schlüssel anlegen) 3,6 s · Leerlauf-Sync 4,1–4,2 s · B erster Sync (4 empfangen,
> 1 gepusht) 2,1 s · B Push 1 Op 9,0 s (Ausreißer, inkl. `devices/*.json`) · A Pull 2 Ops 6,0 s. Folgerung für P7: Kontext
> (Schlüssel, Ordner-Cache) über die Prozesslaufzeit halten – in der App fällt der ~3–4 s-Sockel nur einmal an; Push weiter
> bündeln (Debounce). Isolationsprüfung OK (nur Doc-Kommentar in `todo-core/src/lib.rs`).
> Gefunden dabei (P6-04-Live-Contract): eigene Löschungen fehlten im Feed → e35862a.

---

## P7 · Desktop-App (Tauri 2 + Svelte 5)

Lies vorher: `ui-guidelines.md` vollständig, `architecture.md` §4 (Linux) und §6, ADR-001/003.

### [x] P7-01 · Systemvoraussetzungen & Tauri-Scaffold — 2026-10-17 · 91a8720
**Ziel:** Leere Tauri-App startet auf Arch.
**Schritte:**
1. Pakete: `webkit2gtk-4.1`, `base-devel`, `curl`, `wget`, `file`, `openssl`, `librsvg` (laut aktueller Tauri-Doku prüfen).
2. In `apps/desktop` per `pnpm create tauri-app` (Svelte + TS, Vite) scaffolden; generierte Dateien aufräumen
   (Demo-Code, unnötige Plugins/Capabilities entfernen).
3. `src-tauri` in den Cargo-Workspace aufnehmen; App-ID, Fenstertitel, Mindestgröße setzen.
4. `just desktop-dev` und `just desktop-build` im `justfile`.
**Fertig wenn:** `just desktop-dev` öffnet ein Fenster; Release-Build läuft.
> Notiz: Pakete waren vorhanden (webkit2gtk-4.1 2.54.1, librsvg, openssl …). Scaffold von Hand statt `pnpm create tauri-app`
> (interaktiv, Demo-Code) – gleiches Ergebnis: Vite 8 + Svelte 5.57 + TS 6.0 (TS 7 noch nicht von svelte-check/typescript-eslint
> unterstützt), Tauri 2.12 ohne Plugins, Capability nur `core:event:default`, CSP gesetzt. Crate `todo-desktop`, App-ID
> `dev.tasks.app`, Fenster 1180×760, min. 480×400. `desktop-build` = `tauri build --no-bundle` (Packaging nach dem Prototyp);
> Release-Binary 4,6 MB. Dev-Fenster nach ~6 s (warm). Eigenes Icon (`src-tauri/icon.svg` → `tauri icon`).

### [x] P7-02 · Spike S-4: Performance-Messung mit 1 000 Zeilen — 2026-10-18 · 2ec0a9a
**Ziel:** ADR-001 mit echten Zahlen bestätigen, **bevor** die UI gebaut wird.
**Schritte:**
1. Temporäre Seite: 1 000 Dummy-Zeilen (einfache virtualisierte Liste), Scrollen.
2. Messen (Release-Build): Kaltstart bis erster Paint, PSS nach 60 s Leerlauf (`smem -t -P '<binary>|WebKit'`),
   Idle-CPU (`pidstat`), subjektives Scroll-Verhalten. Wenn möglich auf Intel/AMD **und** NVIDIA.
3. Messskript in `tools/measure-desktop.sh` ablegen (wiederverwendbar für P8).
4. Ergebnis in ADR-001 eintragen. Budgets verfehlt → User informieren, GTK4-Ausweg diskutieren, **nicht** einfach weitermachen.
**Fertig wenn:** Zahlen dokumentiert; Budgets eingehalten oder Entscheidung mit User getroffen. Temporäre Seite entfernt.
> Notiz: Zahlen in ADR-001 (AMD/Mesa, KDE; NVIDIA/Intel nicht verfügbar). Start ~350 ms ✔, Idle-CPU ~0 % ✔, Scrollen
> 60 fps ✔, **PSS 266 (X11) – 299 MB (Wayland) ✗** (alt: hart 180). WebKit-Schalter ohne verlässlichen Effekt. User-Entscheidung:
> Tauri bleibt, RAM-Budget → Ziel < 250 / hart < 320 MB (`AGENTS.md` §4). `tools/measure-desktop.sh` misst ohne smem/pidstat
> direkt aus `/proc` (Prozessbaum, wartet auf vollständiges Beenden – sonst halbiert eine Rest-Instanz den PSS scheinbar).
> Dauerhaft geblieben: Command `app_ready` (erster Frame → Startzeit; P7-03 hängt den Scheduler-Start daran).

### [x] P7-03 · Composition Root & Tauri-Commands — 2026-10-18 · 357bbbc
**Ziel:** Rust-Seite verdrahtet Core, SQLite, Sync, Proton.
**Schritte:**
1. `state.rs`: `CoreApi`, `SyncEngine`, Scheduler in Tauri-State; DB unter `$XDG_DATA_HOME/<app-id>/tasks.db`.
2. `commands.rs`: dünne `async`-Commands 1:1 zu `CoreApi`-Methoden; Fehler als serialisierbares Enum.
3. `events.rs`: `CoreEvent` → `core://changed`, `SyncStatus` → `core://sync-status`.
4. Scheduler an Tokio-Timer und Fenster-Events (Fokus, versteckt) anschließen.
5. Tauri-Capabilities minimal halten (nur eigene Commands).
6. DTO-TypeScript-Typen generieren (`specta` + `tauri-specta` oder `ts-rs`) nach `packages/core-client/src/types.ts`.
**Fertig wenn:** Commands per Dev-Konsole aufrufbar; Typen generiert; Startzeit nicht verschlechtert (Messskript).
> Notiz: Details in ADR-019. Commands per `invoke` in der echten App unter der ACL geprüft (Smoke-Seite: CRUD, beide
> Events, `not_a_command` → „not allowed by ACL", Schlüsselbund verfügbar) und dauerhaft per IPC-Test mit
> `tauri::test::MockRuntime` (`src-tauri/tests/commands.rs`); Scheduler-Treiber mit Fake-Runner + pausierter tokio-Zeit
> getestet. Typen: `ts-rs` (dev-dep) statt specta (nur RC) → `packages/core-client/src/generated/*.ts`, `types.ts`
> re-exportiert. Startzeit 354 ms (vorher 349 ms). Release-Binary jetzt 20 MB (Proton-Adapter/rpgp; vorher 4,4 MB) – in P8-03
> bewerten. `setup` läuft erst mit der Event-Loop → State-Aufbau als `manage_state` (auch für Tests). Netzwechsel kommen aus
> der WebView (`online`/`offline` → `set_online`), „versteckt" = minimiert/unsichtbar bei Fokusverlust/Resize oder Schließen.
> Inbox-ID ist bereits deterministisch (`…0001`) – relevant für P8-01.

### [x] P7-04 · `packages/core-client` — 2026-10-18 · 1a6ccb2
**Ziel:** UI-seitige Abstraktion.
**Schritte:**
1. pnpm-Paket mit `CoreClient`-Interface (`interfaces.md` §7) und `TauriCoreClient` (`invoke`, `listen`).
2. `FakeCoreClient` (In-Memory, TS) für UI-Entwicklung im Browser und für Komponententests.
3. ESLint-Regel: `@tauri-apps/*` nur in `tauri-core-client.ts`.
**Fertig wenn:** Unit-Tests (Vitest) für `FakeCoreClient` grün; Typecheck grün.
> Notiz: Paket `@tasks/core-client` ohne Build-Schritt (Vite liest die TS-Quellen); `TauriCoreClient` als Subpath
> `@tasks/core-client/tauri`, damit die Web-App Tauri nie bündelt. Interface ergänzt um `auth.info/openBrowser/cancelLogin`,
> `sync.status/now`, `ready()` (siehe `interfaces.md` §7). Ablehnungen einheitlich als `CoreClientError { kind }`.
> `FakeCoreClient`: Core-Semantik (feste Inbox-ID, Titel-/Namensregeln, Reihenfolge inkl. before/after/zwischen, Ansichten
> Heute/Demnächst/Erledigt, Status-Filter, Cursor-Seiten, Events erst nach der Antwort, simulierter Login + Sync-Status),
> 10 Vitest-Tests. ESLint-Flat-Config im Root (TS + Svelte); `just ts` (Lint, tsc/svelte-check, Vitest) läuft in `just check`.

### [x] P7-05 · `packages/ui`: Tokens, Theme, Basiskomponenten — 2026-10-18 · b55744a
**Ziel:** Design-System-Grundlage.
**Schritte:**
1. `tokens.css` laut `ui-guidelines.md` §3; Theme-Umschaltung System/Hell/Dunkel; Theme-Snippet im `<head>` (kein Weiß-Blitz).
2. Komponenten: `Button`, `IconButton`, `Checkbox` (rund), `TextInput`, `Toast`, `VirtualList`.
3. Lucide-Icons einzeln importieren.
4. Einfache Demo-Seite (nur Dev) zum Sichten aller Komponenten in beiden Themes.
**Fertig wenn:** Demo-Seite zeigt alle Komponenten in Hell/Dunkel; Kontrast stichprobenartig geprüft (≥ 4.5:1).
> Notiz: Kontrast nicht stichprobenartig, sondern als Test aller Token-Paare (46 Vitest-Fälle) – fand 6 zu schwache Paare
> (u. a. Warn-/Erfolgsfarbe, weißer Text auf hellem Violett/Rot im Dunkel-Theme) → Werte angepasst, `ui-guidelines.md` §3.
> Demo-Seite per Headless-Chrome gesichtet (beide Themes nebeneinander). Theme-Snippet als Inline-Script über Vite-Plugin;
> im Release-Build bestätigt, dass Tauri die CSP-Hashes ergänzt und `data-theme` vor dem ersten Paint steht. Offen (klein):
> Das GTK-Fenster selbst ist vor dem ersten WebView-Paint kurz in Systemfarbe. Tooltips nur CSS (`data-tooltip`).

### [x] P7-06 · App-Layout & Sidebar — 2026-10-18 · 5eb047b
**Ziel:** Grundgerüst wie in `ui-guidelines.md` §2.
**Schritte:**
1. `App.svelte` mit Sidebar (Primärbutton „Neue Task", Smart Views, Listen, Sync-Status-Fußzeile) und Hauptbereich.
2. Mini-Router (Hash- oder State-basiert) für Views/Listen.
3. Responsive Verhalten (Sidebar einklappbar).
**Fertig wenn:** Navigation zwischen Views funktioniert (mit `FakeCoreClient` und mit Tauri).
> Notiz: `App.svelte` liegt in `packages/ui` (für Desktop und später Web), bekommt den `CoreClient` als Prop. Layout nach
> aktuellem Proton-Muster: Hintergrund in Sidebar-Farbe, Hauptbereich als abgerundete Fläche. Ansichten: Eingang, Heute,
> Geplant, Alle Tasks, Erledigt; Zähler nur für Eingang/Heute (zählt seitenweise, entprellt – eigene Count-Abfrage im Core
> wäre besser, siehe FIX-04). Nachweis FakeCoreClient: 4 Komponententests (jsdom). Nachweis Tauri: **Dev-Selbsttest**
> (`VITE_TASKS_SELFTEST=1 pnpm tauri dev`, `apps/desktop/src/selftest.ts`) klickt in der echten WebView per DOM und zeigt
> PASS/FAIL als Overlay – OS-Eingaben (xdotool) erreichen das XWayland-Fenster unter KWin nicht zuverlässig und landeten
> womöglich in einem anderen Fenster (dem User gemeldet), WebKitWebDriver/Xvfb sind nicht installiert. Gefunden: Race nach
> „Liste anlegen" (Fallback hielt die neue Liste für gelöscht) → behoben. JS-Bundle 28 KB gzip.

### [x] P7-07 · Task-Liste — 2026-10-18 · 2d9a3ee
**Ziel:** Kernansicht.
**Schritte:**
1. `TaskList.svelte` (Container) lädt seitenweise über `CoreClient`, reagiert auf `onChange` gezielt per ID.
2. `TaskRow.svelte`: Checkbox, Titel, Due-Badge, Priorität; Hover-Aktionen; erledigte Tasks gedimmt/durchgestrichen.
3. `$state.raw` für die Liste; Virtualisierung über `VirtualList`.
4. Optimistisches Abhaken mit Rückgängig-Toast.
**Fertig wenn:** 1 000 Tasks scrollen flüssig; Abhaken fühlt sich sofort an.
> Notiz: Gemessen per Dev-Selbsttest in Tauri (WebKitGTK, Dev-Build, 1 000 Tasks über IPC in 2,4 s angelegt): 21 DOM-Zeilen,
> Scrollen 371 Frames p50 16 / p95 21 / max 23 ms, 0 Drops; Abhaken: Haken nach 1 ms im DOM, nächster Frame nach 12 ms.
> Änderungen per ID: geänderte Task nachladen, per Ansichtsprädikat (`view-match.ts`, spiegelt `TaskQuery::matches`)
> ein-/aus-/einsortieren; Gelöschte (NotFound) entfernen. Abgehakte Tasks bleiben bis zum Ansichtswechsel gedimmt stehen
> (Rückgängig per Klick oder Toast). Ansichten zeigen offene Tasks, „Erledigt" die erledigten. Zeilen sind `option`s einer
> `listbox` mit eigenem `aria-label` (Titel, Status, Fälligkeit). 9 neue Vitest-/Komponententests.

### [x] P7-08 · Quick-Add & Bearbeiten — 2026-10-18 · 31d2152
**Ziel:** Tasks schnell erfassen und ändern.
**Schritte:**
1. Quick-Add-Zeile oben (Enter = anlegen, bleibt fokussiert). Parser zunächst nur für einfache Daten
   („heute", „morgen") – Rest nach dem Prototyp.
2. Detail-Panel rechts: Titel, Notizen, Fälligkeit (einfacher Date-Input reicht im Prototyp), Priorität, Liste.
3. Speichern bei Blur/Debounce, nicht per Button.
**Fertig wenn:** Anlegen, Bearbeiten, Verschieben in andere Liste funktioniert und überlebt Neustart.
> Notiz: Parser: `heute`/`morgen`/`übermorgen` als ganzes Wort (letztes zählt), wird aus dem Titel entfernt, live als Chip
> angezeigt; in „Heute" ist heute vorbelegt. Anlegen mit Fälligkeit = `createTask` + `updateTask` (zwei Commits – für den
> Prototyp ok; ein `due` in `CreateTaskDto` spart später einen). Detail-Panel: Felder mit Label darüber (die Prioritäts-
> Segmente passen so in 360 px), Speichern bei Blur bzw. 600 ms Pause, ausstehende Eingaben werden beim Wechsel/Schließen
> gespeichert. Nachweis: 7 Komponententests + Tauri-Selbsttest in zwei Läufen auf derselben Datenbank (`p708a` anlegen/
> bearbeiten/verschieben → Neustart → `p708b` prüft Liste, Notiz, Priorität, Fälligkeit). Gefunden: kurzer Fehler-Toast bei
> unbekannter Liste vor dem Fallback → Liste wird erst gerendert, wenn sie existiert.

### [x] P7-09 · Sortieren & Löschen — 2026-10-18 · 54fe367
**Ziel:** Reihenfolge ändern, Tasks entfernen.
**Schritte:**
1. Drag & Drop in der Liste → `moveTask` mit Nachbar-IDs; Tastatur `Alt+↑/↓`.
2. Löschen per `Entf`/Menü mit Rückgängig-Toast.
**Fertig wenn:** Reihenfolge bleibt nach Neustart erhalten.
> Notiz: Drag & Drop mit Pointer-Events statt HTML5-DnD (zuverlässiger in WebKitGTK und in der virtualisierten Liste),
> Griff links bei Hover, Einfügelinie, Auto-Scroll. Sortieren nur in Eingang/Listen. **Löschen sofort** im Core;
> „Rückgängig" legt die Task neu an (gleiche Felder, Position über Nachbarn) – neue ID, weil Tombstones nicht wiederbelebbar
> sind; verzögertes Löschen hätte bei Absturz die Löschung verloren. Kontextmenü (Rechtsklick) als „Menü".
> Nachweis: 6 Komponententests; Tauri-Selbsttest `p709a` (Alt+↑ ×3, Ziehen mit echtem Layout, Entf) → Neustart → `p709b`
> Reihenfolge S4 S3 S1. Gefunden: Fokus ging nach dem Umsortieren verloren (Svelte verschiebt den DOM-Knoten) → wird
> wiederhergestellt; Klick fokussiert die Zeile explizit.

### [x] P7-10 · Tastaturbedienung — 2026-10-18 · 4adbd7f
**Ziel:** Shortcuts aus `ui-guidelines.md` §5 (Prototyp-Teilmenge: `N`, `↑/↓`, `J/K`, `Leertaste`, `Enter`, `Entf`, `Esc`).
**Schritte:**
1. Zentrales `shortcuts.ts` (eine Registry, kein verstreutes `keydown`); Kollision mit Texteingaben vermeiden.
2. Sichtbarer Fokus.
**Fertig wenn:** Kompletter Ablauf „anlegen → abhaken → löschen" ohne Maus möglich.
> Notiz: `packages/ui/src/lib/shortcuts.ts` (Registry, Kombinationen + Folgen, Sperre bei modalem Dialog). Zusätzlich
> zur Teilmenge: `Strg+N` auch in Textfeldern, `E`, `Alt+↑/↓` (aus P7-09 übernommen), `G I/T/U`, `?` (Übersicht aus der
> Registry). Auswahl ist jetzt von „Detail offen" getrennt (Pfeile wählen nur, offenes Panel folgt). Sichtbarer Fokus:
> WebKits `:focus-visible` greift bei programmatischem Fokus nicht verlässlich → `data-input=keyboard|pointer` auf `<html>`,
> Fokusring bei Tastaturbedienung immer. Gefunden: `instanceof HTMLSummaryElement` warf (Klasse existiert nicht) → Leertaste/
> Enter hätten global eine Exception ausgelöst; Esc schloss den Kürzel-Dialog nur über das native `cancel`. Nachweis:
> 7 Vitest-/Komponententests + Tauri-Selbsttest `p710` (N → tippen/Enter ×2 → Esc → ↓/J → Leertaste → Enter/Esc → J+Entf → ?).

### [x] P7-11 · Login-Flow & Sync-Status in der UI — 2026-10-18 · 046a9fc
**Ziel:** Proton-Anmeldung aus der App.
**Schritte:**
1. Login-View (lazy geladen) rendert generisch nach `AuthStep`: Zugangsdaten → 2FA → zweites Passwort.
2. Human Verification: Challenge-URL in separatem Fenster/Browser öffnen, Token zurückübergeben (Weg aus P2 übernehmen).
3. App ist **ohne** Login voll nutzbar (lokal); Login über Einstellungen/Sidebar-Fußzeile.
4. Sync-Status-Anzeige (Icon + Tooltip mit letzter Synchronisation / ausstehenden Änderungen); Abmelden.
**Fertig wenn:** Login mit Testkonto inkl. 2FA klappt; Status-Anzeige wechselt korrekt (Syncing/Idle/Offline).
> Notiz: Abweichung (ADR-013): kein Zugangsdaten/2FA/zweites-Passwort-Formular – Login per Browser-Session-Fork, die
> Login-Seite des Anbieters erledigt Passwort, 2FA und Human Verification. Die UI rendert generisch `openBrowser → pending →
> done` im Dialog „Einstellungen" (lazy, eigener Chunk 3,9 KB gzip): Countdown, erneut öffnen, Link kopieren, abbrechen;
> angemeldet: Konto, Status, letzter Sync, ausstehende Änderungen, „Jetzt synchronisieren", Abmelden mit Bestätigung.
> Pflichthinweis + Anbietername aus der Composition Root. **Live-Nachweis 2026-10-18** (User meldete sich im Browser an,
> Dev-Build, frische DB): Selbsttest `p711live` alle PASS – Statusverlauf syncing → idle → offline (WebView-`offline`-
> Ereignis) → syncing → idle; Daten aus P6 wurden gezogen. Beobachtet: Liste „Arbeit" und zwei Tasks doppelt – stammen aus
> den zwei CLI-Datenbanken des P6-05-Live-Tests (gleiche Namen, verschiedene IDs), kein Fehler der App → P8-01 betrachtet
> Duplikate beim ersten Login. Sitzung liegt jetzt im Schlüsselbund unter App-ID `dev.tasks.app`.

---

## P8 · Integration & Abnahme

### [x] P8-01 · Lokale Daten beim ersten Login — 2026-10-18 · d8f8cc3
**Ziel:** Vor dem Login angelegte Tasks gehen nicht verloren.
**Schritte:**
1. Test: Tasks offline anlegen → einloggen → Sync → zweites Gerät sieht sie.
2. Zweites Gerät hatte ebenfalls lokale Tasks → nach Login sind beide Bestände vereint (keine Duplikate der Inbox! –
   Inbox-ID-Strategie prüfen, ggf. deterministische Inbox-ID pro Konto).
**Fertig wenn:** Beide Szenarien funktionieren manuell; Inbox existiert genau einmal.
> Notiz: Kein Code-Fehler gefunden – feste Inbox-ID (P1-07) + LWW + `TaskId`-Tiebreak reichen. Nachweis automatisiert:
> `crates/todo-sync/tests/first_login.rs` (SQLite + Verzeichnis-Backend: offline → erster Sync → frisches Gerät; zwei
> lokale Bestände → Vereinigung, 1 Inbox, Umsortieren über gleiche Sort-Keys; Einstieg per Snapshot mit lokalen Daten).
> Manuell mit `todo-cli` (zwei DBs, ein Sync-Ordner): Inbox `…0001` einmal, alle 4 Tasks auf beiden. Gleichnamige Listen
> bleiben bewusst zwei Listen (ADR-020; Zusammenführen später als explizite Aktion). Gefunden: Desktop-Profile über
> `TODO_DATA_DIR` teilten sich Sitzung + Sync-Schlüssel im Schlüsselbund → eigener Eintrag pro Profil (nötig für P8-02).
> Live-Lauf über Proton ist Teil des P8-02-Protokolls.

### [x] P8-02 · Zwei-Geräte-Abnahmetest — 2026-10-18 · 288ec00
**Ziel:** Prototyp-Definition erfüllt.
**Schritte:** Mit zwei Rechnern (oder zwei Profilen über `XDG_DATA_HOME`) manuell durchspielen und protokollieren:
1. Anlegen auf A → erscheint auf B (nach Fokus/Sync).
2. Beide offline (Netz trennen): A ändert Titel, B Fälligkeit derselben Task; A löscht Task X, B bearbeitet X;
   beide sortieren um → wieder online → beide Geräte identisch und gemäß Merge-Regeln.
3. App während Sync hart beenden → Neustart → keine Datenverluste/Duplikate.
4. Session abgelaufen/abgemeldet → App bleibt lokal nutzbar, Status „Anmeldung erforderlich".
**Fertig wenn:** Protokoll in `development/prototype-acceptance.md`; alle Punkte bestanden oder als Bug-Aufgabe unten angehängt.
> Notiz: Abweichung „manuell" → **automatisiert mit der echten App** (Eingaben am Desktop sind tabu, Proton-Login nur durch
> den User): `tools/acceptance-two-devices.sh` startet zwei Profile (`TODO_DATA_DIR`) im Debug-Build gegen ein gemeinsames
> Verzeichnis (`TODO_REMOTE_DIR`, nur Debug; `src-tauri/src/dev.rs`, mit Latenz `TODO_REMOTE_DELAY_MS` und simulierter
> abgelaufener Sitzung `.auth-revoked`), Suiten `p802*` im Selbsttest zur Laufzeit (`TASKS_SELFTEST`, Bericht auf stdout).
> Alle 4 Punkte bestanden, keine Bugs (Protokoll Teil A). `kill -9` nachweislich mitten im Push (Outbox 1 001 ausstehend)
> und im Start-Sync danach. Beobachtet: periodischer Pull nach 120 s statt 60 s, weil ein ruhiger Lauf den Abstand
> verdoppelt – so vorgesehen. Live-Teil über Proton (Teil B, Checkliste im Protokoll) braucht den User → FIX-06.

### [x] P8-03 · Performance-Abnahme — 2026-10-18 · 674617a
**Ziel:** Budgets aus `AGENTS.md` §4 mit echter App.
**Schritte:**
1. Testdatenbank mit 1 000 Tasks (über `todo-cli` erzeugen).
2. `tools/measure-desktop.sh`: Kaltstart, PSS nach 60 s Idle, Idle-CPU über 5 min (inkl. eingeloggtem Sync), Binary-Größe.
3. Ergebnisse in `architecture.md` §6 eintragen.
**Fertig wenn:** Budgets eingehalten oder Abweichungen mit Ursachen + Folgeaufgaben dokumentiert.
> Notiz: Ergebnisse in `architecture.md` §6 „Abnahme Desktop". Start 382 ms ✔, Idle-CPU 0,02 % über 5 min ✔, Binary 20 MB,
> PSS **306 MB** (hart ✔, Ziel 250 ✘) – erst nach Abschalten des WebKit-Compositing-Modus beim Start (vorher 315–336 MB;
> Varianten-Messreihe + Sichtprüfung Wayland/X11 + Scroll-/Abhak-Messung in ADR-001). Abweichungen: gemessen **ohne**
> Anmeldung (eigenes Profil, damit keine Testdaten ins Proton-Konto gehen) → angemeldete Messung in FIX-06; Ziel 250 MB → FIX-07.
> FIX-03 bei 1 000 Tasks nicht messbar, bleibt offen. FIX-05 damit für „nicht angemeldet" erledigt; angemeldet kann es knapp über 320 MB liegen → FIX-06/07.

### [x] P8-04 · Aufräumen & Dokumentation — 2026-10-18 · ef40618
**Ziel:** Sauberer Stand für die nächste Etappe (Web).
**Schritte:**
1. `development/`-Dokumente mit Realität abgleichen (Interfaces, Struktur, ADRs).
2. `AGENTS.md`: Status auf „Prototyp" setzen, Kommandos aktualisieren.
3. `spikes/` entfernen oder archivieren; Proton-Testsitzungen beenden.
4. Offene Punkte als neue Aufgaben in einer Folge-Roadmap sammeln (siehe unten).
**Fertig wenn:** `just check` grün, Doku stimmt, Commit `chore: prototype milestone`; optional Git-Tag `v0.1.0-prototype`.
> Notiz: Doku-Abgleich per Prüf-Agent (~65 Abweichungen in `module-structure`, `interfaces`, `architecture`,
> `tech-decisions`, `ui-guidelines`, `sync-strategy`) – alle eingearbeitet (Ist-Stand, Geplantes markiert, Spikes S-1/S-2/S-4
> als erledigt, ADR-013 „bestätigt", Logging-Lücke dokumentiert). `AGENTS.md`: Status „Prototyp", Kommandos/Werkzeuge,
> Umgebungsvariablen, Verweis auf `roadmap-next.md`. `spikes/` gelöscht, Probe-Quellen + Herkunft der Referenz-Klone in
> `development/archive/`. **Proton-Testsitzungen nicht beendet** (Abmelden betrifft das Konto des Users) → Teil von
> N0-01 in `roadmap-next.md`. Folge-Roadmap: `development/roadmap-next.md` (N0 Abrunden, N1 Web, N2 Funktionen).

---

## Nach dem Prototyp (Kandidaten, noch nicht geordnet)

- Web-App: `todo-wasm`, Web-Worker, Speicher-Spike S-3, CORS-Weg aus ADR-013.
- Tombstone-Bereinigung, Full-Resync nach langer Offline-Zeit (offen aus P5-11):
  - Tombstones älter als 90 Tage beim Schreiben eines Snapshots weglassen (lokal erst nach Snapshot-Bestätigung löschen).
  - Gerät > 90 Tage offline (`inactive_device_ms`): beim nächsten Start Full-Resync vom neuesten Snapshot, lokale
    ungepushte Ops danach erneut anwenden; bis dahin holt es nur per Snapshot-Join auf (P5-11), was Tombstone-Bereinigung
    noch nicht verträgt.
- Tags, Subtasks, Volltextsuche (FTS5), Befehlspalette, natürliche Datumseingabe.
- Benachrichtigungen für Fälligkeiten.
- Packaging: `PKGBUILD` (AUR), später Flatpak.
- i18n (DE/EN).
- Android-Prototyp (siehe `future-android.md`).

## Bugs / Nachträge während der Abarbeitung

_(Hier neue Aufgaben anhängen, die beim Abarbeiten entstehen – mit ID `FIX-nn`.)_

- [ ] FIX-01 · CI-Workflow (`just check`) anlegen, sobald ein Git-Remote (GitHub/Codeberg) existiert. _(aus P0-04)_
- [x] FIX-02 · Tiebreak für gleiche `SortKey`s nach parallelem Einfügen auf zwei Geräten festlegen (z. B. nach `TaskId`) – spätestens in P5-04. _(aus P1-06, ADR-011)_ — 2026-10-17 · af363e9
  > Anzeige: `sort_key`, dann `TaskId` (seit P3). `MoveTask` verteilt bei gleichen Nachbar-Keys den Lauf gleicher Keys
  > ab dem oberen Nachbarn neu (normale `Set`-Ops) statt mit `Validation` abzubrechen. Szenario-Test: P5-09 #3.
- [ ] FIX-03 · `CreateTask`/`MoveTask` laden die Zielliste komplett (O(n), ~7 ms bei 10 000 Tasks, P4-04).
  Repository-Methode für letzten Sort-Key bzw. Nachbar-Keys ergänzen (In-Memory + SQLite + Contract-Test), wenn
  Listen dieser Größe realistisch werden oder spätestens vor P8-Messungen. _(aus P3-06/P4-04)_
- [ ] FIX-04 · Sidebar-Zähler (Eingang/Heute) zählen über `listTasks`-Seiten (bis 999). Eine `count_tasks(query)`-Abfrage
  in Repository/CoreApi (In-Memory + SQLite + Contract-Test) spart IPC bei großen Listen. _(aus P7-06)_
- [x] FIX-05 · PSS der fertigen App (Release, angemeldet, Sync aktiv, kleine DB) **326–330 MB > hart 320 MB** (2026-10-18,
  nach P7-11): App 136 MB (Spike 111–119; + Proton-Krypto/HTTP/SQLite), WebProcess 162 MB (Spike 146–151), Network 36 MB.
  Start 370 ms ✔, Idle-CPU 0 % ✔, Binary 20 MB. Ansatzpunkte für P8-03: `HardwareAccelerationPolicy::Never` (Spike: bis
  −50 MB, verrauscht), `malloc_trim` nach Sync-Läufen, Proton-Kontext erst beim ersten Sync laden, WebKit-Cache-Modell;
  sonst Budget mit User neu bewerten (ADR-001). _(aus P7-11)_
  > Versuch 2026-10-18: `HardwareAccelerationPolicy::Never` → 199–263 MB, **aber unter X11 schwarzes Fenster** (mit GPU
  > normal) → verworfen. `WEBKIT_DISABLE_COMPOSITING_MODE=1` rendert, Messungen streuen 123–308 MB (Restinstanzen?) →
  > in P8-03 mit sauberer Messreihe (Wayland + X11, Sichtprüfung) erneut bewerten.
  > Erledigt in P8-03 (2026-10-18): Compositing-Modus aus → 306 MB ohne Anmeldung (hart ✔). Angemeldet noch nicht
  > nachgemessen (bisher ~20 MB mehr → voraussichtlich ~326 MB, knapp über hart!) → Messung in FIX-06, Abhilfe in FIX-07.
- [ ] FIX-06 · Live-Teil der Zwei-Geräte-Abnahme über Proton Drive (`prototype-acceptance.md` Teil B: zwei Profile,
  Login im Browser, Offline-Konflikt, `kill -9`, Abmelden/Sitzung beenden) – vom User durchzuspielen, Ergebnis eintragen.
  Dazu die Messung **angemeldet**: im angemeldeten Standardprofil `RUNS=1 IDLE_S=60 CPU_S=300 tools/measure-desktop.sh`
  → PSS/CPU in `architecture.md` §6 nachtragen. _(aus P8-02/P8-03)_
- [-] FIX-07 · (zurückgestellt 2026-10-18, User: RAM ok) RAM-Ziel < 250 MB PSS (aktuell 306 MB, P8-03): App-Prozess analysieren (44 MB glibc-Heap – `heaptrack`,
  `malloc_trim` nach Start/Sync, Proton-/Keyring-Kontext erst bei Bedarf laden), WebKit-Cache-Modell erneut mit stabiler
  Messung prüfen; X11-Schalter (−37 MB) nur als Option. _(aus P8-03)_
