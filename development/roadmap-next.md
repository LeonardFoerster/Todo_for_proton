# Roadmap nach dem Prototyp (Etappe 2: Web)

Fortsetzung der Prototyp-Roadmap (abgeschlossen 2026-10-18, Tag `v0.1.0-prototype`; Aufgaben-IDs wie „P7-08" hier
beziehen sich auf `archive/roadmap-prototype.md`). **Vorher** die offenen Zwischenphasen in `roadmap.md` abarbeiten
(P0–P3 abgeschlossen, siehe `archive/roadmap-interim.md`). N5 (Notes) und N4 stehen als Entwürfe zwischen N0 und N1;
Reihenfolge laut User: N0 → N5 → N4 → N1; **2026-10-21 erweitert** (User: „Fahre mit den anderen Punkten fort“ nach
der Bestandsaufnahme): N6 (Papierkorb + Export) → N7 (Erinnerungen) → N4 (wiederkehrend, freigegeben) → N8 (Checklisten +
Tags) → N9 (Verknüpfung Task ↔ Notiz) → N10 (Bedienung) → N11 (AUR-Paket) → N1. Gleiche Regeln:
**erste offene Aufgabe** nehmen, Vorab-Lektüre lesen, vollständig abarbeiten, abhaken mit Datum + Commit-Hash,
Abweichungen als `> Notiz:`, Entscheidungen als ADR. Die Aufgaben sind noch grob – vor dem Start einer Gruppe in
Einzelaufgaben mit „Schritte/Fertig wenn" schneiden (wie in `roadmap.md`).

## N0 · Prototyp abrunden (vor der Web-Etappe)

### [x] N0-01 · Live-Abnahme über Proton (FIX-06) — 2026-10-21 · vom User abgenommen
> Notiz: User 2026-10-21: „Die Live-Abnahme ist alright.“ (vorher 2026-10-18: „Die Synchronisation zwischen zwei
> Geräten funktioniert“). Die Einzelzeilen in `prototype-acceptance.md` B wurden nicht einzeln protokolliert.
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

## N5 · Notes (freigegeben 2026-10-20)

> **Status:** Vom User am 2026-10-20 freigegeben und vor N0 gezogen (N0-01 braucht den Menschen am Rechner).
> Entscheidungen dabei: Konfliktkopie heißt „<Name> (Konflikt <TT.MM.JJJJ>)" ohne Gerätenamen (Datum der unterlegenen
> Revision, UTC, auf allen Geräten gleich); `ResolveNoteHeads` läuft **in** `apply_remote` (eine Transaktion). Reihenfolge laut User: **N0 → N5 → N4 → N1** (das Web bringt Notes direkt mit).

**Ziel:** Zweiter Modus „Notes": `.txt`/`.md`-Notizen in verschachtelten Ordnern, gleiche Seitenleiste, großer
Editor, Wechsel per Klick auf „Tasks"/„Notes" bzw. `Strg+1/2`, Sync im Op-Log mit 3-Wege-Merge (ADR-029).
**Vorab-Lektüre für alle N5-Aufgaben:** `notes.md` vollständig, ADR-015, ADR-016, ADR-022, ADR-027.

### [x] N5-01 · ADR-029 + Domain `Note`, `NoteFolder`, Baum-Regeln — 2026-10-20 · b0e5118
**Lies vorher:** `notes.md` §3, `domain/task.rs`, `domain/task_list.rs`, `domain/sort_key.rs`, `domain/ids.rs`.
**Schritte:** IDs, `NoteName` (Validierung), `NoteKind`, `Note`/`NoteFolder` mit `*Parts`/`from_parts`, Auto-Name aus
erster Zeile, reine Baum-Funktionen (Waisen → Wurzel, Zyklus-Bruch per höchster HLC, wirksamer Papierkorb, Tiefe ≤ 32).
`AGENTS.md` §5 Domain-Sprache um `Note`, `NoteFolder` ergänzen. ADR-029 auf „akzeptiert" (Teil Domain).
**Fertig wenn:** Unit- und Property-Tests (Baum ist immer zyklenfrei, jedes Element erreichbar), `just check` grün.
Commit `feat(core): note domain`.

### [x] N5-02 · Revisionen + 3-Wege-Merge — 2026-10-20 · 1bf91a0
**Lies vorher:** `notes.md` §4.2–4.5, `sync/merge.rs`, `sync/hlc.rs`.
**Schritte:** `sync/revision.rs`: `RevisionSet` (Vereinigung, Köpfe, jüngster gemeinsamer Vorfahr), `merge3`,
`resolve_body` (Ergebnis: Text + optional unterlegener Kopf), deterministische Konfliktkopie-ID (UUIDv5). diff3-Crate
prüfen (`diffy`: Größe Release-Binär und WASM vorher/nachher, wasm32-Build), sonst eigener Myers + diff3. Regel für
benachbarte Zeilen festlegen und testen.
**Fertig wenn:** Property-Tests aus `notes.md` §4.8 (Konvergenz, merge3-Eigenschaften), `just check` + `just wasm`
grün, Größen im Commit genannt. Commit `feat(core): note revisions and three-way merge`.

### [x] N5-03 · Sync-Format, Outbox-Zusammenfassen, Engine — 2026-10-20 · 4ddb16a
**Lies vorher:** `sync/op.rs`, `sync/replica.rs`, `todo-sync` (Push, Scheduler, Snapshot), ADR-017, ADR-022.
**Schritte:** `EntityRef::Note`/`NoteFolder`, neue `FieldName`-Codes, `OpKind::Revise` (alles angehängt);
Snapshot-Format um Revisionen erweitert (Köpfe + gemeinsame Vorfahren); Outbox ersetzt eine ungepushte `Revise` derselben
Notiz; Scheduler mit eigener Höchstwartezeit für Inhalte (30 s) und Sofort-Auslösern; Segment-Limit 2 MiB Klartext;
nach `apply_remote` `ResolveNoteHeads` auslösen. Golden File `ops_v1_notes.bin`, `ops_v1.bin` unverändert.
**Fertig wenn:** Szenario-Tests aus `notes.md` §4.8 grün; Test „unbekannte Variante → Segment übersprungen, kein
Absturz"; `just check` grün. Commit `feat(sync): notes in the op log`.
> Notiz: Tabellen und `ReplicaStore` für SQLite schon hier (ohne sie kompiliert der Workspace nicht), ebenso der
> Lese-Port `NoteRepository` mit `get_note`/`get_note_folder` für die Contract-Tests. Szenarien zusätzlich als
> Contract-Fälle mit zwei Repositories (`testing/note_sync_contract.rs`, für beide Speicher) und über die Engine
> (`todo-sync/tests/notes.rs`). Details ADR-029, Nachtrag N5-03.

### [x] N5-04 · SQLite: Tabellen, `NoteRepository`, FTS, Aufräumen — 2026-10-20 · ae3f27b
**Lies vorher:** `notes.md` §3.5, §4.5, §7, Migrationen, `mapping.rs`, Repository-Contract, ADR-014, ADR-027.
**Schritte:** Migration (`note_folders`, `notes`, `note_revisions`, `notes_fts` mit `WHEN`-Triggern); `ChangeSet` um
Notiz-/Ordner-Änderungen erweitern; Lese-Port `NoteRepository` (Ordnerinhalt seitenweise, Notiz laden, Papierkorb,
`search_notes`, Revisionen) mit Standard-Implementierungen + In-Memory; Revisionen nach §4.5 aufräumen. Optional eigener
Commit: `TaskRepository` → `Repository` umbenennen (mechanisch).
**Fertig wenn:** Contract-Tests für beide Implementierungen, `bench_storage` um Notizen erweitert (Budgets §9), `just
check` grün. Commit `feat(storage): notes`.

### [x] N5-05 · Use-Cases — 2026-10-20 · 58ebafd
**Lies vorher:** `notes.md` §3–4, `app/` (Muster `create_task.rs`, `move_task.rs`, ADR-024 Reihenfolge-Sperre).
**Schritte:** alle Use-Cases aus `notes.md` §10 inkl. `UpdateNoteBody { id, text, base_rev }` → `{ rev, body }`,
Leere neue Notiz beim Verlassen verwerfen, Auto-Leeren nach 30 Tagen, `ResolveNoteHeads`. `CoreEvent::Changed`
um `notes`/`noteFolders` erweitern.
**Fertig wenn:** Unit-Tests je Use-Case, Zwei-Geräte-Szenarien (gleichzeitige Bearbeitung mit sauberem Merge und mit
Konflikt, Papierkorb vs. Bearbeitung, Leeren vs. neue Notiz), `just check` grün. Commit `feat(core): note use cases`.
> Notiz: `ResolveNoteHeads` ist kein eigener Use-Case, sondern Teil jedes Commits/Pulls (User-Entscheidung, ADR-029
> Nachtrag N5-03). Kleine Use-Cases teilen sich Dateien (`edit_note.rs`: Umbenennen + Typ; `move_note_items.rs`;
> `trash_notes.rs`: Papierkorb + Wiederherstellen; `purge_notes.rs`: Löschen, Leeren, 30 Tage, Papierkorb-Liste).
> `base_rev` muss die zuletzt gelieferte Revision sein (Speichern je Notiz nacheinander), sonst kann eine ersetzte,
> noch nicht hochgeladene Revision nicht als veraltet erkannt werden. Notizen im (wirksamen) Papierkorb sind nur
> lesbar: Bearbeiten, Umbenennen, Typ, Verschieben → `Validation`. `CoreEvent::Changed` hat seit N5-03 `notes`/
> `note_folders`.

### [x] N5-06 · API, Shell, `CoreClient` — 2026-10-20 · 27886dd
**Lies vorher:** `api/dto.rs`, `commands.rs` (Auslöser-Liste), `core-client.ts`, `fake-core-client.ts`, `interfaces.md` §7.
**Schritte:** DTOs (`NoteSummaryDto` ohne Inhalt, `NoteDto`, `NoteFolderDto`, `NoteTreePageDto`), Methoden je
Use-Case, Commands + Auslöser + Capabilities, Fake-Client mit gleicher Semantik, `just types`; `todo-wasm` kompiliert
weiter.
**Fertig wenn:** IPC-Tests (Speichern → `onChange`; Remote-Merge liefert geänderten `body`), Auslöser-Test, Vitest
für den Fake, `interfaces.md` §7 ergänzt, `just check` grün. Commit `feat(desktop): note commands`.

### [x] N5-07 · UI: Modus-Umschalter + Notiz-Baum + Papierkorb — 2026-10-20 · 10b4ceb
**Lies vorher:** `notes.md` §5, `Sidebar.svelte`, `App.svelte`, `VirtualList.svelte`, ui-guidelines §2/§5/§7.
**Schritte:** `lib/app-mode.svelte.ts` (Modus, Persistenz), Brand-Button mit `Strg+1/2` und Palette-Einträgen,
Notes-Lazy-Chunk, Tasks-Teilbaum bleibt gemountet; `NotesSidebar`/`NoteTree` (virtualisiert, lazy Ordner,
Tastatur, Drag & Drop, Kontextmenü, Umbenennen inline, Neue Notiz/Ordner); Papierkorb-Ansicht; leerer Zustand.
**Fertig wenn:** Vitest (Umschalten behält Zustand beider Modi, Baum-Tastatur, Verschieben, Papierkorb +
Rückgängig), ui-guidelines §2/§5 ergänzt, `just check` grün. Commit `feat(ui): notes mode and note tree`.

### [x] N5-08 · UI: Editor — 2026-10-20 · 4120757
**Lies vorher:** `notes.md` §6.1–6.4, §6.6.
**Schritte:** `NoteEditor` (textarea, Schrift je Typ, Zeilenbreite), Autosave (§6.2), Remote-Änderung in den Puffer
(§6.3), `editor-commands.ts` (Listen, Einrücken, Format-Kurzbefehle über `insertText`; WebKitGTK prüfen),
Statuszeile, Zen-Modus, Einstellungsabschnitt „Notizen".
**Fertig wenn:** Vitest für `editor-commands` (Tabelle mit Cursor-Positionen) und Autosave/Remote-Fälle,
Interaktion < 16 ms (p95 im Selbsttest), `just check` grün. Commit `feat(ui): note editor`.
> Notiz: `insertText` in WebKitGTK und die Interaktionszeit prüft der Selbsttest in N5-12 (jsdom kennt
> `execCommand` nicht; dort greift der Ersatz über `setRangeText`). Wird während eines Speicherns weitergetippt und
> liefert der Core einen gemergten Text, überträgt `rebase` die eigene Änderung (ein Bereich); überschneiden sich
> beide, wird von der alten Basis aus erneut gespeichert (Core mergt oder legt eine Kopie an, nichts geht verloren).
> Schließen: Die Shell hält `CloseRequested` an, sendet `app://close-requested`, die UI speichert und meldet
> `close_ready` (sonst schließt das Fenster nach 1,5 s). `Strg+Alt+Ziffer` über `KeyboardEvent.code` (AltGr).

### [x] N5-09 · UI: Markdown-Vorschau — 2026-10-20 · 206e4fe
**Lies vorher:** `notes.md` §6.5.
**Schritte:** `markdown/render.ts` (Umfang laut §6.5, DOM statt `innerHTML`), anklickbare Checklisten,
Links extern über die Shell, Scroll-Abgleich, `Strg+E`.
**Fertig wenn:** Vitest mit Spezifikationsfällen + XSS-Fällen, Größe des Renderers ≤ 15 KB gzip, Render-Budget
gemessen, `just check` grün. Commit `feat(ui): markdown preview`.
> Notiz: Renderer ~4 KB gzip (unminifiziert), Notes-Chunk gesamt 18,7 KB JS + 2,3 KB CSS gzip (Budget 25 KB).
> Render-Budget: 20 KB in jsdom als Plausibilitätstest (< 80 ms); die echte Messung in WebKit macht der Selbsttest
> (N5-12). Neues Command `open_external` (nur http/https/mailto, `xdg-open`). Weiche Zeilenumbrüche im Absatz werden
> wie in CommonMark zu Leerzeichen; harte Umbrüche mit zwei Leerzeichen oder `\` am Zeilenende.

### [x] N5-10 · Gemeinsame Suche — 2026-10-20 · 66f1a1c
**Lies vorher:** `notes.md` §7, `features/palette/`, ADR-027.
**Schritte:** Palette fragt `searchTasks` und `searchNotes` parallel ab, Gruppen, Ausschnitt, Öffnen im Notes-Modus
mit Sprung zum Treffer.
**Fertig wenn:** Vitest, Messung p95 bei 2 000 Notizen, `just check` grün. Commit `feat(ui): search notes`.
> Notiz: Messung `bench_storage` (2 000 Notizen à 20 KB): `CoreApi::search_notes` mit Pfad p95 8 ms, Einzelsuchen
> p95 0,1–14,8 ms. Ohne Treffer legt Enter nur im Modus „Tasks" eine neue Task an.

### [x] N5-11 · Ordner-Import (Desktop) — 2026-10-20 · db8972a
**Lies vorher:** `notes.md` §8.
**Schritte:** Ordnerauswahl (Portal vs. `tauri-plugin-dialog`, Größe prüfen, im Commit begründen), Einlesen in der
Shell, `ImportNotes` in Blöcken, Ergebnis-Dialog.
**Fertig wenn:** Integrationstest mit Testordner (Unterordner, versteckt, Nicht-UTF-8, zu groß, Symlink, CRLF/BOM),
`just check` grün. Commit `feat(desktop): import notes from a folder`.

> Notiz: Ordnerauswahl über das XDG-Portal (`ashpd`, Feature `file_chooser`; Crate war schon im Baum) statt
> `tauri-plugin-dialog`. Grenzen in der Shell: höchstens 20 000 Dateien, 31 Unterordner-Ebenen unter dem neuen Ordner.
> Im Web fehlt `notes.importFolder`, der Menüeintrag erscheint dort nicht.

### [~] N5-12 · Selbsttest, Messung, Abnahme, Doku — 2026-10-21 · fc8db74, d758c2b (User-Abnahme offen)
**Schritte:** WebView-Selbsttest `notes` (umschalten, Notiz anlegen, tippen, Autosave, Vorschau, zurück zu Tasks mit
erhaltenem Zustand, Interaktion bis Repaint); Abnahmeskript „Notiz auf zwei Geräten" (sauberer Merge + Konflikt);
`tools/measure-desktop.sh` im Notes-Modus (RAM-Zuwachs, Start im Notes-Modus) → `architecture.md` §6; Doku
(`sync-strategy.md`, `interfaces.md`, `ui-guidelines.md`, `module-structure.md`, README mit Hinweis „alle Geräte
aktualisieren", AGENTS-Status); Abnahme durch den User (Checkliste in `prototype-acceptance.md`).
**Fertig wenn:** Selbsttest + Abnahmeskript grün, Budgets aus `notes.md` §9 eingehalten oder als FIX erfasst,
User-Abnahme eingetragen. Commit `test(desktop): notes acceptance`.

> Notiz: Automatisch alles grün – Selbsttests `notes` (12 Schritte) und `n5seed` in WebKitGTK, Abnahmeskript Abschnitt
> 8 (`ONLY=notes`) und der volle Lauf aller 8 Abschnitte. Alle Budgets aus `notes.md` §9 eingehalten (Messwerte
> `architecture.md` §6 „Notes"); kein neuer FIX. Der Selbsttest fand zwei Fehler, behoben in 021d725 (Vorschau
> sprang nach einer Fokus-Bitte sofort zurück) und 0d481cf (`app_ready` kam im Notes-Modus von der versteckten
> Task-Liste). `measure-desktop.sh` kann mit `MODE`/`NOTES_OPEN` im Notes-Modus starten (eigenes Profil, vorbelegter
> WebView-localStorage). Wartet auf die Abnahme durch den User: Checkliste `prototype-acceptance.md` E.

## N6 · Papierkorb für Tasks + Export/Import

**Ziel:** Nichts geht mehr versehentlich verloren, und die Daten lassen sich jederzeit herausholen. Löschen legt Tasks in
einen Papierkorb (30 Tage, wie bei Notizen), „Rückgängig" behält die IDs. Export: alle Listen und Tasks als JSON, Notizen
als `.md`/`.txt` in ihrer Ordnerstruktur; die JSON lässt sich wieder importieren.

**Für alle Aufgaben gilt:** Neue Felder beim Materialisieren mit **Standardwert**, wenn das Register fehlt
(`merge.rs::materialize_task`, sonst verschwinden bestehende Tasks). Ältere Apps überspringen Segmente mit neuen Feldern
(„App aktualisieren“) – alle Geräte gemeinsam aktualisieren. Contract-Tests für In-Memory **und** SQLite.

### [x] N6-01 · ADR-031 + Core: Tasks in den Papierkorb — 2026-10-21 · 3c01484
> Notiz: Kein neuer Feldcode – `Trashed` (16) gilt jetzt auch für Tasks. Die Papierkorb-Methoden der `CoreApi` kamen
> schon hier (für das Szenario `trash_and_concurrent_edit_both_survive`). FIX-13 mit erledigt.
**Lies vorher:** `notes.md` §Papierkorb, ADR-029, `app/trash_notes.rs`, `app/purge_notes.rs`, `sync/op.rs` (`Trashed`),
`app/delete_task_list.rs`, `ports/repository.rs` (`TaskQuery::matches`), `todo-storage-sqlite/src/query.rs`.
**Schritte:**
1. ADR-031: Papierkorb = LWW-Feld `Trashed` (Code 16, jetzt auch für Tasks), Wiederherstellen = `Trashed = None` mit
   **gleicher ID**, endgültig = `Delete`; Aufbewahrung 30 Tage; Liste löschen nimmt Papierkorb-Tasks mit in den Eingang.
2. `Task.trashed_at` (+ Parts, Getter, `set_trashed`), `FieldName::TASK` um `Trashed`, `task_fields`, `materialize_task`
   mit Standard `None`; Property-Test um „in den Papierkorb/zurück“ erweitern; Golden für das neue Task-Feld.
3. `SmartView::Trash` (angehängt); alle anderen Ansichten und die Suche blenden Papierkorb-Tasks aus (`TaskQuery::matches`,
   SQLite `query.rs`, Standard-`search_tasks`); `DeleteTaskList` verschiebt auch Papierkorb-Tasks.
4. Use-Cases `TrashTasks`, `UntrashTasks`, `PurgeTasks` (nur aus dem Papierkorb), `EmptyTaskTrash`, `PurgeExpiredTasks`.
5. Migration (nächste Nummer): `tasks.trashed_at INTEGER NULL` + Teilindex; Mapping.
6. FIX-13: `NotifyingRepository` leitet `search_tasks` an das Repository weiter (bisher O(n)-Standard statt FTS5).
**Fertig wenn:** `just check` grün; Szenario „A legt in den Papierkorb, B bearbeitet“ konvergiert (Bearbeitung bleibt,
Task im Papierkorb).

### [x] N6-02 · API, Shell, Client — 2026-10-21 · 84227f0
**Schritte:** DTOs (`trashedAt`, `ViewDto::trash`), Commands `trash_tasks`, `untrash_tasks`, `purge_tasks`,
`empty_task_trash` (+ Auslöser-Liste, Capabilities, IPC-Test), tägliches Aufräumen zusammen mit den Notizen
(`purge_expired_notes_daily`); `CoreClient`, Tauri-Client, Fake (gleiche Semantik), `view-match.ts`.
**Fertig wenn:** `just check` grün, `interfaces.md` ergänzt.

### [x] N6-03 · UI: Löschen = Papierkorb, Ansicht „Papierkorb“ — 2026-10-21 · d98dca1
> Notiz: Kein Detail-Panel im Papierkorb (statt „nur lesend“): Klick setzt nur den Cursor, Wiederherstellen/Endgültig
> löschen an Zeile, Kontextmenü und Auswahlleiste. Ausgeblendete IDs werden nach Erfolg freigegeben (`release`), damit
> „Rückgängig“ mit gleichen IDs die Zeilen per Event zurückbringt.
**Schritte:** `Entf`, Kontextmenü, Detail und Auswahl legen in den Papierkorb (Toast „In den Papierkorb“ + Rückgängig =
`untrash`, gleiche IDs, keine Neuzuordnung von Auswahl/Cursor); Seitenleiste „Papierkorb“ unten im Modus „Tasks“ mit
Zahl; Ansicht mit „gelöscht vor 3 Tagen“, Wiederherstellen, Endgültig löschen (Bestätigung), `⋯` → „Papierkorb leeren“;
Detail im Papierkorb nur lesend mit „Wiederherstellen“.
**Fertig wenn:** Vitest (Löschen → Papierkorb → Wiederherstellen, Leeren, Liste löschen), Selbsttest.

### [x] N6-04 · ADR-032 + Export/Import — 2026-10-21 · 67f3aac
> Notiz: Rundlauf-Test über echte Dateien in `apps/desktop/src-tauri/tests/export.rs` (statt Selbsttest – die
> Portal-Dialoge lassen sich in der WebView nicht automatisch bedienen). Notizen zurück über „Ordner importieren…“.
**Schritte:**
1. ADR-032: Exportformat `tasks-export` v1 (JSON: `format`, `version`, `exportedAt`, `lists`, `tasks` mit allen Feldern
   inkl. Papierkorb); Import legt fehlende Listen (über Namen) und Tasks **mit neuen IDs** an (eine alte ID könnte auf
   einem anderen Gerät schon gelöscht sein – „Delete gewinnt“ würde sie wieder entfernen) und überspringt Tasks, die es
   schon gibt (gleicher Titel, gleiche Liste, gleiches `createdAt`); Notizen als Dateien, Import über „Ordner
   importieren“.
2. Core: `export_data()`, `import_data(dto)` → Bericht (angelegt/übersprungen); in Stücken ≤ 500 je Commit.
3. Shell: „Exportieren…“ wählt per Portal einen Ordner und schreibt `Tasks-Export-JJJJ-MM-TT/tasks.json` +
   `notizen/…`; „Importieren…“ wählt eine `tasks.json`. Nur Desktop.
4. UI: Einstellungen → „Daten“: Exportieren…, Aus Export wiederherstellen… (Ergebnisdialog wie beim Ordner-Import);
   Palette-Einträge.
**Fertig wenn:** Rundlauf-Test (Export → leeres Profil → Import → gleiche Listen/Tasks/Notizen), Selbsttest.

---

## N7 · Erinnerungen + Hintergrund

**Ziel:** Die App meldet sich zur Fälligkeit, auch wenn das Fenster zu ist. Mit Uhrzeit zur Uhrzeit, ganztägige um
09:00 (einstellbar). Schließen lässt die App im Tray weiterlaufen; Autostart optional. Kein Polling: ein einziger Timer
auf die nächste Erinnerung, neu berechnet bei Änderungen, Fokus und Aufwachen.

### [x] N7-01 · ADR-033 + Planer — 2026-10-21 · 58a96ee
**Schritte:** ADR-033 (nur lokal, nicht synchronisiert; jedes Gerät meldet selbst; schon gemeldet = je Gerät gemerkt,
`(task, due)` in einer kleinen Datei im Datenordner; verpasste Erinnerungen beim Start/Aufwachen zusammengefasst, höchstens
24 h zurück). Reine Planungsfunktion (nächster Zeitpunkt, fällige Meldungen) mit Unit-Tests (ganztägig, Uhrzeit,
erledigt/Papierkorb, Zeitumstellung, Mehrfachmeldung).
### [x] N7-02 · Shell: Benachrichtigung, Tray, Schließen, Autostart — 2026-10-21 · 3c2ba6d
> Notiz: Benachrichtigung über das XDG-Portal (ashpd `notification`) statt direktem D-Bus. Einzelinstanz über einen
> Socket in `$XDG_RUNTIME_DIR` (Name aus dem Datenordner – Socket-Pfade sind auf ~100 Zeichen begrenzt). Fenster
> startet versteckt und wird in `setup` gezeigt (außer `--hidden`). Messung: Binary +0,97 MB seit N5, Start 372 ms,
> PSS 300 MB, Leerlauf-CPU 0 %.
**Schritte:** Desktop-Benachrichtigung über D-Bus (Größe der Abhängigkeit prüfen und im Commit nennen), Klick öffnet die
Task; Tray mit „Öffnen“, „Neue Task“, „Beenden“; Fenster schließen → verstecken, wenn „Im Hintergrund weiterlaufen“ an
(Speichern der Notizen wie bisher); Autostart über `~/.config/autostart/…desktop` mit `--hidden`; Einstellungen in einer
Datei der Shell, Commands zum Lesen/Setzen.
### [x] N7-03 · UI: Einstellungen „Erinnerungen“ — 2026-10-21 · 3c2ba6d
> Notiz: Zusammen mit N7-02 committet. Selbsttests `n7rem` (Erinnerung nach 3,0 s, Verschieben meldet neu, Erledigtes
> nicht) und `n7bg` (Schließen versteckt); Klick auf die echte Desktop-Meldung prüft der User (`prototype-acceptance.md` G).
**Schritte:** Erinnerungen an/aus, Uhrzeit für ganztägige, Im Hintergrund weiterlaufen, Mit dem System starten; Öffnen
einer Task aus der Meldung.
**Fertig wenn (N7):** `just check` grün; Messung: Leerlauf-CPU ~0 %, PSS-Zuwachs im Commit; Selbsttest (Erinnerung in
1 min → Meldung, Klick öffnet die Task); Abnahme durch den User.

---

## N4 · Wiederkehrende Tasks (freigegeben 2026-10-21)

> **Status:** freigegeben 2026-10-21, offene Fragen „wie vorgeschlagen“ entschieden: Verlaufseinträge in „Erledigt“ ja,
> verpasste Vorkommen überspringen, „nach Erledigung“ dabei. Reihenfolge: nach N6/N7. Migration heißt nicht mehr
> `0003_recurrence.sql` (0003 ist die Suche), sondern die nächste freie Nummer. Erinnerungen kommen aus N7.

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

### [x] N4-01 · ADR-028 + Domain `Recurrence` — 2026-10-21 · 29eaa94
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

### [x] N4-02 · Sync-Format und SQLite — 2026-10-21 · 29eaa94
> Notiz: Zusammen mit N4-01 committet (Feld `Recurrence` = Code 17, Golden-Datei `ops_v1_recurrence.bin`, Migration 0006).
**Lies vorher:** `sync/op.rs`, `sync/merge.rs`, `sync/diff.rs`, ADR-016, `crates/todo-storage-sqlite/migrations/`,
`mapping.rs`, Repository-Contract (`testing/`).
**Schritte:**
1. `FieldName::Recurrence` am Ende anhängen (fester `u8`-Code), `FieldValue::Recurrence(…)` angehängt, kompakte
   postcard-Form (Wochentage als Bitmaske, `until` als Tageszahl). `diff.rs`/`merge.rs` (Materialisieren) erweitern.
2. Golden File: `ops_v1.bin` bleibt unverändert lesbar (Test). Neues `ops_v1_recurrence.bin` für die neuen Codes.
3. Migration `000n_recurrence.sql` (nächste freie Nummer): Spalte `recurrence BLOB NULL` an `tasks` (postcard, gleiche Form wie die Op).
   Mapping und Contract-Test (Roundtrip, `None` bleibt `NULL`).
4. Szenario-Test (`todo-sync/tests/scenarios.rs`): Regel auf A setzen → B hat sie; A ändert die Regel, B die Notiz →
   beides bleibt.
**Fertig wenn:** `just check` grün; ein Lauf einer **alten** Binärversion gegen ein Segment mit `Recurrence` zeigt
„App aktualisieren" und kein Absturz (manuell oder Test mit unbekanntem Code). Commit `feat(sync): recurrence field`.

### [x] N4-03 · Use-Cases: Abhaken, Überspringen, Rückgängig — 2026-10-21 · 4f9c050
> Notiz: Mit N4-04 zusammen committet (die neue Signatur von `complete_tasks` baut nur über alle Schichten). `UndoOccurrence` bekommt die alte Fälligkeit mitgeliefert (`previous_due`); nach Rückgängig nimmt erneutes Abhaken die nächste ID-Variante (Tombstone). Szenario „A überspringt, B hakt ab“: Serie am Folgetag, Bs Verlaufseintrag bleibt.
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

### [x] N4-04 · API, Shell, `CoreClient` — 2026-10-21 · 4f9c050
> Notiz: `jiff` war schon für N7 im Binary, keine neue Abhängigkeit. `undoOccurrence(OccurrenceDto)` statt zwei IDs; `completeTask` (einzeln) rechnet mit dem UTC-Datum, die UI nimmt `completeTasks` mit `today`.
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

### [x] N4-05 · Fenster „Neue Task": Wiederholung erkennen — 2026-10-21 · a6f919d
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

### [x] N4-06 · UI: Anzeige, Detail-Panel, Abhaken — 2026-10-21 · 32c2a8c
> Notiz: Der Tooltip am Icon ist allgemein („Wiederholt sich“) – die Zusammenfassung trägt nur `recurring`, die Regel im Klartext steht im Detail. Löschen fragt nicht nach (Papierkorb mit Rückgängig, ADR-031); der Toast nennt die Serie.
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
> Notiz (2026-10-21): Selbsttest `n4rec` grün (Abhaken bis Repaint p50 16 / p95 18 ms – ein Frame bei 60 Hz; geprüft wird wie überall die harte Grenze 50 ms), Abnahme-Skript Abschnitt 9 grün, Doku nachgezogen, installiert. Messung Release: Start 360 ms, PSS 311 MB (hart 320), Leerlauf-CPU 0 %, Binary 23 214 600 B (+138 KB seit N7). Wartet auf die User-Abnahme (`prototype-acceptance.md` H). Die offenen Fragen unten sind vom User am 2026-10-21 beantwortet (Verlauf ja, verpasste überspringen, „nach Erledigung“ ja, N4 nach N0-01).
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

## N8 · Checklisten + Tags

### [x] N8-01 · ADR-034 + Checkliste an der Task — 2026-10-21 · 77b2ab1
> Notiz: Verschieben per `Alt+↑/↓` und Griff; das nächste Vorkommen einer Serie beginnt ohne Haken.
**Schritte:** Feld `Checklist` (neuer Code, Liste aus `{id, text, done}`, LWW auf die ganze Liste wie Tags in v1, max.
100 Punkte), Standard leer beim Materialisieren; API/DTO; Detail-Panel: Punkte hinzufügen/abhaken/umbenennen/löschen/
ziehen, `Enter` neuer Punkt; Zeile/Karte: „2/5“; Suche findet Punkttext.
### [x] N8-02 · ADR-035 + Tags als eigene Entität — 2026-10-21 · b7ad6c9
> Notiz: Tag-ID aus dem Namen abgeleitet (UUIDv5) – offline doppelt angelegt = ein Tag. Export enthält die Tags, Import ordnet über den Namen zu.
**Schritte:** `Tag { id, name, sort_key }` als neue synchronisierte Entität (`EntityRef::Tag` angehängt, wie
`TaskList`); `Task.tags: Vec<TagId>` (Feld existiert, Code `Tags`) bekommt Logik; Umbenennen/Löschen eines Tags;
`TaskQuery.tag` filtert; API/DTO/Commands.
### [x] N8-03 · UI: Tags — 2026-10-21 · a67095b
> Notiz: Chip „Tags“ mit `Alt+T`; neue Tasks in einer Tag-Ansicht bekommen den Tag. Selbsttest `n8` (Tag + Checkliste) grün; Szenario-Tests in `todo-sync/tests/scenarios.rs`.
**Schritte:** `#tag` im Fenster „Neue Task“ (markiert wie die anderen Angaben, unbekannte Tags werden angelegt), Chip
„Tags“, Tags im Detail und in Zeile/Karte; Seitenleiste „Tags“ mit Ansicht je Tag; Palette.
**Fertig wenn (N8):** `just check` grün, Szenario-Tests (Tag umbenennen auf A, zuweisen auf B), Selbsttest.

---

## N9 · Verknüpfung Task ↔ Notiz

### [x] N9-01 · ADR-036 + `Link` als Entität — 2026-10-21 · 72df9f9
> Notiz: ID aus beiden Enden (UUIDv5); verwaiste Verknüpfungen räumt `purge_dangling_links` täglich mit dem Papierkorb auf.
**Schritte:** `Link { id, from: Task, to: Note }` (N3: eigenes Element statt Felder an `Task`, später auch `External`),
nur anlegen/löschen; verschwindet ein Ende, wird der Link nicht angezeigt (und beim Aufräumen entfernt).
### [ ] N9-02 · UI
**Schritte:** Detail-Panel „Notizen“ (verknüpfen über Suche, öffnen, lösen); Notiz-Statuszeile „Tasks“ (verknüpfte Tasks,
neue Task aus der Notiz); Icon in Zeile/Karte.

---

## N10 · Bedienung

### [ ] N10-01 · Sortieren und Filtern
**Schritte:** Pro Ansicht sortieren (Eigene Reihenfolge, Fälligkeit, Priorität, Titel; gemerkt pro Gerät), Filter-Chips
(Priorität, Tag) in der Kopfzeile; Core-Abfrage mit Sortierschlüssel statt O(n) im Client.
### [ ] N10-02 · Zähler über `count_tasks` (FIX-04)
**Schritte:** `count_tasks(query)` in Port/SQLite/API; Seitenleiste und Unterzeile der Kopfzeile auch für Listen/Geplant.
### [ ] N10-03 · Gleichnamige Listen zusammenführen (ADR-020)
**Schritte:** Aktion „Mit … zusammenführen“ im Listenmenü (Tasks verschieben, Liste löschen, ein Commit) + Hinweis nach
dem ersten Login, wenn es gleichnamige Listen gibt.
### [ ] N10-04 · Erste Schritte
**Schritte:** Leerer Eingang beim ersten Start mit kurzer Einführung (Texterkennung, Kürzel, Anmelden für Sync),
ausblendbar.

---

## N11 · Paket für Arch Linux (AUR)

### [ ] N11-01 · `PKGBUILD`
**Schritte:** `packaging/aur/PKGBUILD` (+ `.desktop`, Icons, Lizenz), baut aus dem Git-Stand mit `cargo`/`pnpm`;
lokal mit `makepkg` geprüft (ohne Installation); Veröffentlichen im AUR macht der User.

---

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
- ~~Tags, Subtasks~~ → N8 (Checklisten statt Unteraufgaben, User 2026-10-21); Volltextsuche/Palette erledigt (P4-05).
- ~~Benachrichtigungen für Fälligkeiten~~ → N7.
- Packaging: ~~`PKGBUILD` (AUR)~~ → N11, später Flatpak.
- i18n (DE/EN).
- Messung auf NVIDIA/Intel (P8-03 nur AMD/Mesa).
- Android-Prototyp (siehe `future-android.md`).

## N3 · Langfristig: persönliches Werkzeug (im Hinterkopf, nicht jetzt)

Richtung laut User (2026-10-18): Die App wird primär ein persönliches Tool. Kalender und GitHub **ausdrücklich nicht
jetzt**, aber bei Entscheidungen mitdenken (z. B. keine Felder wie `github_issue` an `Task` hängen). Reihenfolge offen.

- **Verknüpfungen** als eigenes, synchronisiertes Domain-Element (`Link { from, to: Task | Note | External { provider,
  ref, title, url } }`) statt Einzelfelder an `Task` – Grundlage für alles Folgende. ADR vor Beginn.
- ~~**Notizen (Uni):** `Note` (Markdown) je Liste …~~ → vorgezogen als **N5** (`notes.md`, ADR-029); geblieben ist
  hier nur die `[[`-Verknüpfung von Notizen mit Tasks (über `Link`, s. o.).
- **Proton Calendar:** keine offizielle API. Stufe 1 nur lesen über die ICS-Freigabe-URL (URL in den Schlüsselbund),
  Termine in Heute/Woche neben den Tasks. Stufe 2 (schreiben, „Zeit blocken") nur nach Spike, ausschließlich im
  `proton-adapter` hinter einem Port.
- **GitHub-Fenster:** eigene Ansicht (zugewiesene Issues, Review-Anfragen), „als Task übernehmen" mit Verknüpfung.
  Eigenes Crate hinter einem Port, Fine-grained Token im Schlüsselbund, nur lokaler Cache (nicht über Proton), Abruf
  bei Öffnen/Fokus mit ETag – kein Polling im Leerlauf.

## Bugs / Nachträge

_(Neue Aufgaben mit ID `FIX-nn` hier anhängen; Nummerierung gemeinsam mit `roadmap.md` – nächste freie Nummer steht
dort, aktuell FIX-12.)_
