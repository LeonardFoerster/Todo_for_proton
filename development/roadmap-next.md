# Roadmap nach dem Prototyp (Etappe 2: Web)

Fortsetzung der Prototyp-Roadmap (abgeschlossen 2026-10-18, Tag `v0.1.0-prototype`; Aufgaben-IDs wie „P7-08" hier
beziehen sich auf `archive/roadmap-prototype.md`). **Vorher** die offenen Zwischenphasen in `roadmap.md` abarbeiten
(P0–P3 abgeschlossen, siehe `archive/roadmap-interim.md`). N4 steht als Entwurf zwischen N0 und N1. Gleiche Regeln:
**erste offene Aufgabe** nehmen, Vorab-Lektüre lesen, vollständig abarbeiten, abhaken mit Datum + Commit-Hash,
Abweichungen als `> Notiz:`, Entscheidungen als ADR. Die Aufgaben sind noch grob – vor dem Start einer Gruppe in
Einzelaufgaben mit „Schritte/Fertig wenn" schneiden (wie in `roadmap.md`).

## N0 · Prototyp abrunden (vor der Web-Etappe)

### [ ] N0-01 · Live-Abnahme über Proton (FIX-06)
Teil B in `prototype-acceptance.md` mit zwei Profilen durchspielen (Mensch am Rechner, Testkonto) und eintragen;
dazu die Messung **angemeldet** (`RUNS=1 IDLE_S=60 CPU_S=300 tools/measure-desktop.sh` im angemeldeten Profil) →
`architecture.md` §6. Funde als FIX-Aufgaben unten anhängen. Danach Testsitzungen beenden: in jedem Profil
Einstellungen → Abmelden, `todo-cli logout`, im Proton-Konto unter Sicherheit → Sitzungen prüfen (aus P8-04 Schritt 3).

### [-] N0-02 · RAM-Ziel < 250 MB (FIX-07) — zurückgestellt 2026-10-18 (User: „Der RAM-Verbrauch ist okay")
Ist 306 MB (nicht angemeldet), angemeldet vermutlich ~326 MB (> hart 320). App-Prozess: 44 MB glibc-Heap (`heaptrack`),
`malloc_trim` nach Start/Sync, Keyring-/Proton-Kontext erst beim ersten Sync bzw. Login laden; WebKit-Cache-Modell mit
stabiler Messreihe (3 Läufe je Variante) erneut prüfen. Kein X11-Zwang (nur als Schalter dokumentieren).

### [ ] N0-03 · Kleinere Nachträge
- ~~`CreateTaskDto.due` – Fälligkeit in einem Commit~~ erledigt mit P3-03 (648e684; auch `notes`, `priority`).
- Sidebar-Zähler über eine `count_tasks(query)`-Abfrage statt Seiten (FIX-04).
- „Listen zusammenführen" als explizite Aktion (gleichnamige Listen nach dem ersten Login, ADR-020).
- GTK-Fenster vor dem ersten WebView-Paint kurz in Systemfarbe (P7-05).
- ~~FIX-03~~ erledigt (3ffd69d); Rest FIX-11 (O(n) bei `move_task` mit Nachbarn), sobald Listen > 5 000 Tasks realistisch werden.
- ~~CI-Workflow mit `just check` (FIX-01)~~ verworfen 2026-10-18 (User: keine CI-Pipeline).
- ~~Logging in der Desktop-Shell: `tracing`-Subscriber (Level `info`, ohne Inhalte/Tokens) statt `eprintln!`, damit
  Adapter-Logs ankommen (Lücke laut `architecture.md` §5).~~ erledigt 2026-10-18 · 95ff2bf (`TODO_LOG`, vor N0-01
  gezogen, damit die Live-Abnahme Adapter-Logs hat).

## N4 · Wiederkehrende Tasks (Entwurf 2026-10-18 – wartet auf Freigabe)

> **Status:** Entwurf, vom User angefragt (2026-10-18). Alle Aufgaben stehen auf `[~]`, bis der User den Entwurf und
> die offenen Fragen unten freigibt; dann auf `[ ]` setzen. Vorgeschlagene Reihenfolge: **nach N0-01, vor N1**.

**Ziel:** Eine Task kann sich wiederholen („Übungsblatt abgeben jeden Fr", „Vorlesung nachbereiten jeden Mo und Do",
„Miete monatlich am 1."). Abhaken schiebt die Task auf das nächste Vorkommen und legt ein erledigtes Vorkommen als
Verlauf an. Das funktioniert offline auf mehreren Geräten ohne Duplikate.

**Modell (Vorschlag, wird in N4-01 als ADR-028 festgehalten):**
> Notiz (2026-10-19): ADR-Nummer von 025 auf 028 korrigiert – 025 ist das Sicherheits-Audit, 026/027 kamen mit P4.
- **Eine Task = eine Serie.** Es gibt keine vorab erzeugten Instanzen. Die Task trägt ein neues Feld `recurrence`
  und ihre `due` ist immer das **nächste offene Vorkommen**. „Geplant" und „Heute" zeigen nur dieses Vorkommen,
  zukünftige werden nicht hochgerechnet.
- **Regel = kleine RRULE-Teilmenge** (1:1 abbildbar auf iCalendar, wichtig für einen späteren Kalender, N3):
  `Recurrence { freq: Daily | Weekly | Monthly | Yearly, interval: u16 (≥ 1), weekdays: Vec<Weekday>` (nur Weekly,
  leer = Wochentag der `due`), `month_day: Option<u8>` (nur Monthly; 31 → letzter Tag des Monats),
  `anchor: Schedule | Completion, until: Option<NaiveDate> }`.
  - `Schedule`: fester Rhythmus ab der `due`.
  - `Completion`: Rhythmus ab dem Tag des Abhakens („alle 3 Tage nach Erledigung").
- **Abhaken einer wiederkehrenden Task** ist **ein** Commit mit zwei Teilen:
  1. **Verlaufseintrag:** eine neue, erledigte Task als Kopie des Vorkommens (Titel, Notiz, Liste, Priorität, die
     alte `due`, `recurrence: None`). Ihre ID wird **deterministisch** aus `(Serien-ID, Datum des Vorkommens)`
     abgeleitet (UUIDv5-Schema; Ausnahme von „UUIDv7 an allen Grenzen", im ADR begründen). Haken zwei Geräte offline
     dasselbe Vorkommen ab, entsteht dieselbe ID, und `Create` auf eine bestehende Entität mergt feldweise
     (`merge.rs`). So gibt es keine Duplikate.
  2. **Fortschreiben der Serie:** `due` wird auf das nächste Vorkommen gesetzt, der Status bleibt `Open`. Das ist
     ein normales LWW-`Set` und deterministisch, deshalb konvergieren beide Geräte auf denselben Wert.
- **Verpasste Vorkommen** (`Schedule`): Das nächste Vorkommen ist das erste nach der alten `due`, das **≥ heute**
  ist. Wer zwei Wochen zu spät abhakt, bekommt also den nächsten kommenden Termin und keine Kette überfälliger.
  „Heute" liefert der Aufrufer als `LocalDay`, weil der Core keine Zonen rechnet.
- **Ende:** Liegt das nächste Vorkommen nach `until`, wird die Serie selbst erledigt. Es entsteht kein Verlaufseintrag
  zusätzlich, die Serie *ist* dann das letzte Vorkommen.
- **Uhrzeit-Fälligkeiten** (`Due::DateTime`): Das nächste Vorkommen hat dieselbe **lokale Uhrzeit** in `tz`, auch über
  die Zeitumstellung hinweg. Dafür bekommt der Core einen neuen Port `TimeZones` (lokal ↔ UTC für einen IANA-Namen).
  Die Desktop-Implementierung liegt in der Shell (System-tzdb, z. B. `jiff`, Größe prüfen). Ohne Port, etwa im Web
  vor N1, gilt `Validation` für Serien mit Uhrzeit; reine Datums-Serien gehen immer.
- **Bekannte Grenze (im ADR):** Hakt Gerät A offline zweimal ab und Gerät B einmal, kann LWW die `due` auf B's
  (früheren) Wert setzen, falls B's HLC höher ist. Das ist selten und sichtbar, und erneutes Abhaken korrigiert es.
  Kein Sonder-Merge.
- **Sync-Format:** neues `FieldName::Recurrence` (Code **angehängt**, ADR-016) und neues `FieldValue`. Ältere Apps
  überspringen solche Segmente mit dem Hinweis „App aktualisieren". Deshalb alle Geräte gleichzeitig aktualisieren
  (Hinweis in den Release-Notizen bzw. im README).

**Bewusst nicht in N4 (Kandidaten für später):** „jeden 2. Dienstag im Monat" (BYSETPOS), „nach n Vorkommen
beenden" (COUNT), Ausnahmen für einzelne Termine, hochgezählte Titel („Übungsblatt 3 → 4"), Verlauf als eigene
Ansicht je Serie (mit Verknüpfungen aus N3 möglich), Erinnerungen.

### [~] N4-01 · ADR-028 + Domain `Recurrence`
**Lies vorher:** `sync-strategy.md` §2, ADR-011, ADR-016, `crates/todo-core/src/domain/due.rs`, `domain/task.rs`.
**Schritte:**
1. ADR-028 „Wiederkehrende Tasks: Serie mit fortgeschriebener Fälligkeit + deterministische Verlaufseinträge"
   (Modell oben, Alternativen: vorab erzeugte Instanzen bzw. neue Task mit Zufalls-ID beim Abhaken, beide mit
   Duplikaten bei Offline-Abhaken auf zwei Geräten).
2. `domain/recurrence.rs`: Typ mit Validierung (`interval` 1–999, `weekdays` nur bei Weekly, `month_day` 1–31 nur bei
   Monthly, `until` ≥ erste `due`). Reine Funktion `next_on_or_after(&self, previous: NaiveDate, min: NaiveDate) ->
   Option<NaiveDate>` (bzw. `after_completion(done_on)` für `Completion`). Für Datum-Zeit arbeitet sie auf
   `NaiveDateTime` (Wandzeit); die UTC-Umrechnung macht der Use-Case über den Port.
3. Port `TimeZones` in `ports/` (owned Types, FFI-freundlich): `to_local(Timestamp, &str) -> Option<NaiveDateTime>`,
   `to_utc(NaiveDateTime, &str) -> Option<Timestamp>` (bei der Zeitumstellung: Lücke → später, Doppel → früher).
   In `Deps` optional (`Option<Arc<dyn TimeZones>>`).
4. `Task`: Feld `recurrence: Option<Recurrence>` + `set_recurrence(…, clock)` (ohne `due` → `Validation`, die UI setzt
   vorher das erste Vorkommen). `TaskParts`/`from_parts` erweitern.
5. Tests: Unit pro Frequenz (Monatsende 31. → 28./29./30., Schaltjahr 29.2. jährlich, Wochentage über
   Wochengrenze, `interval` 2/3, `until`), Property-Tests: Ergebnis > `previous`, ≥ `min`, passt zur Regel, monoton.
**Fertig wenn:** `just check` grün, ADR-028 eingetragen. Commit `feat(core): recurrence rule`.

### [~] N4-02 · Sync-Format und SQLite
**Lies vorher:** `sync/op.rs`, `sync/merge.rs`, `sync/diff.rs`, ADR-016, `crates/todo-storage-sqlite/migrations/`,
`mapping.rs`, Repository-Contract (`testing/`).
**Schritte:**
1. `FieldName::Recurrence` am Ende anhängen (fester `u8`-Code), `FieldValue::Recurrence(…)` angehängt, kompakte
   postcard-Form (Wochentage als Bitmaske, `until` als Tageszahl). `diff.rs`/`merge.rs` (Materialisieren) erweitern.
2. Golden File: `ops_v1.bin` bleibt unverändert lesbar (Test). Neues `ops_v1_recurrence.bin` für die neuen Codes.
3. Migration `0003_recurrence.sql`: Spalte `recurrence BLOB NULL` an `tasks` (postcard, gleiche Form wie die Op).
   Mapping und Contract-Test (Roundtrip, `None` bleibt `NULL`).
4. Szenario-Test (`todo-sync/tests/scenarios.rs`): Regel auf A setzen → B hat sie; A ändert die Regel, B die Notiz →
   beides bleibt.
**Fertig wenn:** `just check` grün; ein Lauf einer **alten** Binärversion gegen ein Segment mit `Recurrence` zeigt
„App aktualisieren" und kein Absturz (manuell oder Test mit unbekanntem Code). Commit `feat(sync): recurrence field`.

### [~] N4-03 · Use-Cases: Abhaken, Überspringen, Rückgängig
**Lies vorher:** `app/complete_task.rs`, `app/complete_tasks.rs` (ADR-021), `app/reopen_task.rs`,
`app/restore_tasks.rs`, `app/update_task.rs`, ADR-024 (Reihenfolge-Sperre).
**Schritte:**
1. `CompleteTask`/`CompleteTasks`: Für eine Serie gibt es Verlaufseintrag (deterministische ID, `sort_key` ans Ende der
   Liste) plus Fortschreiben in **einem** `ChangeSet`. Das Ende (`until`) erledigt die Serie. Die Eingabe bekommt
   `today: LocalDay`. Ergebnis enthält je Serie `occurrence: Option<TaskId>` (für Rückgängig).
2. `SkipOccurrence { id, today }`: nur Fortschreiben, kein Verlauf („Dieses Mal überspringen").
3. `UndoOccurrence { series_id, occurrence_id }`: Verlaufseintrag löschen und `due` der Serie auf dessen `due`
   zurücksetzen, ein Commit.
4. `CreateTask`/`UpdateTask`: `recurrence` im Input bzw. Patch (`Some(None)` = Wiederholung beenden). Eine Regel ohne
   `due` setzt `due` auf das erste Vorkommen ab heute.
5. Wieder öffnen eines Verlaufseintrags macht ihn zu einer normalen offenen Task, die Serie bleibt unberührt.
   Löschen der Serie löscht nur die Serie, der Verlauf bleibt.
6. Tests: Unit für alle Pfade inkl. Sammel-Abhaken gemischt (Serie + normal), Zeitumstellung mit Fake-`TimeZones`
   (Europe/Berlin, 29.03./25.10.), ohne Port → `Validation` nur bei Uhrzeit. Szenario-Tests: A und B haken offline
   dasselbe Vorkommen ab → **ein** Verlaufseintrag, gleiche `due`; A überspringt, B hakt ab → Ergebnis dokumentiert und
   stabil.
**Fertig wenn:** `just check` grün. Commit `feat(core): complete, skip and undo recurring tasks`.

### [~] N4-04 · API, Shell, `CoreClient`
**Lies vorher:** `api/dto.rs`, `apps/desktop/src-tauri/src/commands.rs` (Auslöser-Liste, ADR-022),
`packages/core-client/src/core-client.ts`, `fake-core-client.ts`, `interfaces.md` §7.
**Schritte:**
1. DTOs: `RecurrenceDto` (camelCase, `freq`, `interval`, `weekdays: ("mo"|…)[]`, `monthDay?`, `anchor`, `until?`),
   `TaskDto.recurrence`, `TaskSummaryDto.recurring: bool` (Icon in Zeilen ohne volles Laden), `CreateTaskDto` /
   `TaskPatchDto` um `recurrence` erweitert. `completeTask`/`completeTasks` bekommen `today: LocalDayDto` und liefern
   zusätzlich `occurrences: { seriesId, occurrenceId }[]`. Neu: `skipOccurrence(id, today)`,
   `undoOccurrence(seriesId, occurrenceId)`. `just types`.
2. Desktop: `TimeZones`-Adapter in der Shell (Kandidat `jiff` mit System-tzdb, Größe des Release-Binärs vorher/nachher
   messen und im Commit nennen), Commands + Auslöser-Liste + Capabilities; `todo-wasm` kompiliert weiter (ohne Port).
3. Fake-Client mit gleicher Semantik (deterministische ID darf im Fake einfach `${seriesId}:${date}` sein).
4. Tests: IPC-Test (Abhaken einer Serie → zwei geänderte IDs im `onChange`), Auslöser-Test, Vitest für den Fake.
**Fertig wenn:** `just check` grün, `interfaces.md` §7 ergänzt. Commit `feat(desktop): recurring task commands`.

### [~] N4-05 · Fenster „Neue Task": Wiederholung erkennen
**Lies vorher:** `features/new-task/parse.ts`, `NewTaskDialog.svelte`, P3-03 (Archiv).
**Schritte:**
1. Parser: `täglich`, `jeden Tag`, `wöchentlich`, `jede Woche`, `jeden Mo`, `jeden Mo und Do` / `jeden Mo, Do`,
   `alle 2 Wochen`, `alle 3 Tage`, `monatlich`, `jeden Monat`, `jeden 15.`, `jährlich`, `werktags` (Mo–Fr);
   Zusatz `nach Erledigung` → `anchor: Completion`; `bis 20.12.` hinter einer Regel → `until`. Regel ohne Tag: erstes
   Vorkommen ab heute als `due`. Je Art zählt wieder die letzte Angabe.
2. Chip „Wiederholung" (Icon `repeat`, Kürzel `Alt+W`) mit Menü: Keine, Täglich, Werktags, Wöchentlich (am Wochentag
   der Fälligkeit), Monatlich, Jährlich, „Benutzerdefiniert…" (kleiner Dialog: Intervall, Einheit, Wochentage,
   Ende, Bezug). Von Hand gesetzt gilt vor dem Text (wie P3-03).
3. Hinweiszeile: erkannte Regel im Klartext („wöchentlich, Mo und Do").
4. Tests: Parser-Tabelle (inkl. Abgrenzung: „jeden" ohne Regel bleibt im Titel), Dialog-Vitest.
**Fertig wenn:** `just check` grün. Commit `feat(ui): recurrence in the new-task window`.

### [~] N4-06 · UI: Anzeige, Detail-Panel, Abhaken
**Lies vorher:** `features/task-detail/TaskDetail.svelte`, `features/task-list/TaskRow.svelte`,
`features/board/BoardCard.svelte`, `lib/task-actions.ts` (Rückgängig-Toasts), ui-guidelines §4/§5.
**Schritte:**
1. Zeile und Karte: kleines `repeat`-Icon neben der Fälligkeit, Tooltip mit Klartext-Regel; Screenreader-Label
   „wiederholt sich wöchentlich".
2. Detail-Panel: Zeile „Wiederholung" (gleiche Auswahl wie der Chip + „Benutzerdefiniert…", „Beenden").
3. Abhaken einer Serie: Die Zeile bleibt stehen, die Fälligkeit springt (kurze Hervorhebung ≤ 160 ms, nur
   `opacity`/`transform`). Toast „Erledigt – nächstes Mal Fr, 31. Okt." mit „Rückgängig" (`undoOccurrence`).
   Sammel-Abhaken: ein Toast für alles.
4. Kontextmenü/Detail: „Dieses Mal überspringen" (`skipOccurrence`). Löschen einer Serie: Bestätigungstext nennt
   „ganze Serie".
5. „Erledigt" zeigt Verlaufseinträge wie normale erledigte Tasks.
6. Vitest: Icon/Tooltip, Abhaken mit Rückgängig, Überspringen, Beenden, Sammel-Abhaken gemischt.
**Fertig wenn:** `just check` grün, Interaktionsbudget < 16 ms beim Abhaken (p95 im Selbsttest).
Commit `feat(ui): recurring tasks in list, board and detail`.

### [~] N4-07 · Selbsttest, Abnahme, Doku
**Schritte:**
1. WebView-Selbsttest `p4rec`: Serie „Blatt jeden Fr" anlegen → abhaken → Verlauf in „Erledigt", `due` +7 Tage →
   Rückgängig → Zustand wie vorher; Überspringen; Interaktion bis Repaint messen.
2. Abnahmeskript: neuer Abschnitt „Serie auf zwei Geräten" (A und B haken offline dasselbe Vorkommen ab → nach Sync
   ein Verlaufseintrag, gleiche `due` auf beiden).
3. Doku: `ui-guidelines` (Chip, Icon, Toast, Kürzel), `sync-strategy.md` §2 (Feld `Recurrence`, deterministische
   Verlaufs-IDs), `interfaces.md`, README (Funktionen, Kürzel, Hinweis „alle Geräte aktualisieren"), AGENTS-Status.
4. `just desktop-install`; Abnahme durch den User am Desktop (kurze Checkliste in `prototype-acceptance.md`).
**Fertig wenn:** Selbsttest + Abnahmeskript grün, User-Abnahme eingetragen. Commit `test(desktop): recurring tasks
acceptance`.

**Offene Fragen an den User (vor Freigabe):**
1. Verlaufseinträge in „Erledigt" – gewünscht, oder soll Abhaken nur die Fälligkeit weiterschieben (einfacher, aber
   kein Nachweis, welche Blätter abgegeben sind)?
2. Verpasste Vorkommen überspringen (Vorschlag) oder einzeln nachholen (jedes überfällige Vorkommen bleibt offen)?
3. Wird „nach Erledigung" (`anchor: Completion`) gebraucht, oder reicht der feste Rhythmus?
4. Reihenfolge: N4 vor N0-01 (Live-Abnahme) oder danach (Vorschlag)?

## N1 · Web-App (Etappe 2)

Lies vorher: `architecture.md` §4 (Web), ADR-002, ADR-004, Spike S-3, `interfaces.md` §7, `proton-research.md` (CORS).

- **N1-01 Spike S-3:** Speicher im Browser – `rusqlite` auf wasm32 vs. sqlite-wasm (OPFS) vs. IndexedDB: Größe,
  Startzeit, Komplexität → ADR-004 Web-Teil.
- **N1-02 CORS / Login im Web:** Preflight gegen den API-Host von fremder Origin (in P2 offen geblieben), Weg für den
  Session-Fork im Browser festlegen (ggf. `packages/proton-web/`) → ADR.
- **N1-03 `todo-wasm`:** `CoreApi` + Repository-Adapter (aus S-3) als wasm-bindgen-Bindings, inkl. der Sammel-Methoden
  aus P0-01 in `archive/roadmap-interim.md` (`complete_tasks`, `delete_tasks`, `move_tasks`, `restore_tasks`); Budget WASM < 300 KB gzip.
- **N1-04 Web Worker + `WasmCoreClient`:** Core im Worker, `CoreClient`-Implementierung über `postMessage`, Events.
- **N1-05 `apps/web`:** Vite-Einstieg mit `@tasks/ui` (`App.svelte`), Budget JS < 80 KB gzip; Dev-Selbsttest analog Desktop.
- **N1-06 Sync im Web:** `RemoteStore` über den Web-Weg aus N1-02, Scheduler im Worker (Sichtbarkeit, `online`/`offline`).
- **N1-07 Abnahme Web:** Desktop + Browser als zwei Geräte; Budgets aus `AGENTS.md` §4 (Web-Zeilen).

## N2 · Funktionen (Kandidaten, noch nicht geordnet)

- Tombstone-Bereinigung (> 90 Tage, beim Snapshot) und Full-Resync nach langer Offline-Zeit (offen aus P5-11).
- Tags (dann auch `#tag` im Fenster „Neue Task"), Subtasks, Volltextsuche (FTS5), Befehlspalette.
- Benachrichtigungen für Fälligkeiten.
- Packaging: `PKGBUILD` (AUR), später Flatpak.
- i18n (DE/EN).
- Messung auf NVIDIA/Intel (P8-03 nur AMD/Mesa).
- Android-Prototyp (siehe `future-android.md`).

## N3 · Langfristig: persönliches Werkzeug (im Hinterkopf, nicht jetzt)

Richtung laut User (2026-10-18): Die App wird primär ein persönliches Tool. Kalender und GitHub **ausdrücklich nicht
jetzt**, aber bei Entscheidungen mitdenken (z. B. keine Felder wie `github_issue` an `Task` hängen). Reihenfolge offen.

- **Verknüpfungen** als eigenes, synchronisiertes Domain-Element (`Link { from, to: Task | Note | External { provider,
  ref, title, url } }`) statt Einzelfelder an `Task` – Grundlage für alles Folgende. ADR vor Beginn.
- **Notizen (Uni):** `Note` (Markdown) je Liste, `[[`-Verknüpfung mit Tasks. Sync: Feld-LWW + Konfliktkopie statt
  Text-CRDT (Budgets). Editor zuerst `textarea` + Vorschau, CodeMirror 6 nur nachgeladen.
- **Proton Calendar:** keine offizielle API. Stufe 1 nur lesen über die ICS-Freigabe-URL (URL in den Schlüsselbund),
  Termine in Heute/Woche neben den Tasks. Stufe 2 (schreiben, „Zeit blocken") nur nach Spike, ausschließlich im
  `proton-adapter` hinter einem Port.
- **GitHub-Fenster:** eigene Ansicht (zugewiesene Issues, Review-Anfragen), „als Task übernehmen" mit Verknüpfung.
  Eigenes Crate hinter einem Port, Fine-grained Token im Schlüsselbund, nur lokaler Cache (nicht über Proton), Abruf
  bei Öffnen/Fokus mit ETag – kein Polling im Leerlauf.

## Bugs / Nachträge

_(Neue Aufgaben mit ID `FIX-nn` hier anhängen; Nummerierung gemeinsam mit `roadmap.md` – nächste freie Nummer steht
dort, aktuell FIX-12.)_
