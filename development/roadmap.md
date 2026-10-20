# Roadmap: Zwischenphasen

Das ist **keine** Zeitplan-Roadmap, sondern eine **geordnete Arbeitsliste** für Phasen, die vor bzw. zwischen den
Etappen aus `roadmap-next.md` eingeschoben werden. Erledigte Phasen werden entfernt. Die Prototyp-Phasen P0–P8
(abgeschlossen 2026-10-18) stehen mit allen Notizen in `archive/roadmap-prototype.md`; alte IDs wie „P7-08" in
anderen Dokumenten beziehen sich auf diese Archivdatei. Die abgeschlossenen Zwischenphasen P0–P3 (Mehrfachauswahl,
Upload ≤ 5 s, Listen & Kanban, Fenster „Neue Task") liegen in `archive/roadmap-interim.md`.

**Reihenfolge:** Erst die offenen Phasen hier (P4), danach weiter mit `roadmap-next.md` (N0-01).

## Wie diese Liste benutzt wird

- **Immer die erste offene Aufgabe** nehmen (`[ ]`). Nicht springen, außer eine Aufgabe sagt es ausdrücklich.
- `[~]` = **wartet auf den User** (z. B. Spezifikation fehlt): überspringen, nicht raten. Erst wieder aufnehmen, wenn der
  User die Vorgabe geliefert hat (dann auf `[ ]` setzen).
- Vor dem Start: „Lies vorher" prüfen und die genannten Dokumente lesen.
- Eine Aufgabe ist erst fertig, wenn **alle** Punkte unter „Fertig wenn" erfüllt sind – inkl. grüner Tests.
- Danach: Checkbox auf `[x]` setzen, Datum + Commit-Hash dahinter schreiben, z. B. `[x] … — 2026-10-20 · a1b2c3d`.
- Abweichungen vom Plan: kurz unter der Aufgabe als `> Notiz:` festhalten; größere Entscheidungen als ADR in
  `tech-decisions.md`.
- Ein Commit pro Aufgabe (Conventional Commits, Scope = Crate/Paket), sofern nicht anders angegeben.
- Standard-Prüfung nach jeder Aufgabe: `just check`.

## Phasenübersicht

| Phase | Inhalt | Warum an dieser Stelle |
|---|---|---|
| ~~P0–P2~~ | Mehrfachauswahl · Upload ≤ 5 s · Listen & Kanban – **abgeschlossen** 2026-10-18 | → `archive/roadmap-interim.md` |
| ~~P3~~ | Kanban: Umschalt + Mausrad · Fenster „Neue Task" – **abgeschlossen** 2026-10-18 | → `archive/roadmap-interim.md` |
| P4 | Bedienung: Uhrzeit im Detail · Heute/Überfällig · Geplant nach Tagen · Fälligkeit per Kürzel · Suche & Befehlspalette · Erledigt nach Tagen – **umgesetzt** 2026-10-19 (P4-01–06); P4-07 wartet auf den User | UX-Review 2026-10-19; die häufigsten Handgriffe (finden, verschieben, Woche überblicken) vor der Web-Etappe |

## P4 · Bedienung: schneller finden, verschieben, überblicken

**Ziel:** Die häufigsten Handgriffe nach dem Abhaken – eine Task finden, ihre Fälligkeit verschieben, die Woche
überblicken – gehen ohne Scrollen und ohne Maus. Optik bleibt (ruhig, dicht, Proton-nah); es geht um Abläufe.
Herkunft: UX-Review 2026-10-19 (`/frontend-design`, `/ui-ux-pro-max`); Befunde aus dem Code, nicht aus Nutzertests.

**Für alle Aufgaben gilt:** Neue Kürzel in die zentrale Registry (`App.svelte` → `lib/shortcuts.ts`) und damit in
`ShortcutHelp`; Gruppen-Überschriften sind keine Optionen (`role="presentation"`, ↑/↓ überspringt sie); Listen bleiben
virtualisiert (`VirtualList`), keine O(n)-Arbeit pro Tastendruck; leere Abschnitte werden nicht gezeigt; Kontrast über
`tokens.test.ts`. UI-Prüfung in der echten WebView nur per Selbsttest (`apps/desktop/src/selftest.ts`). Nach jeder
Aufgabe `just desktop-install`.

### [x] P4-01 · Uhrzeit in der Detailansicht (Datum ändern verliert die Uhrzeit) — 2026-10-19 · af54e80
> Notiz: `lib/format-date.ts` → `dueFromInputs`, `dueToInputTime`, `keepTime` (Dialog „Neue Task" nutzt sie jetzt
> auch; dort behalten die Schnellwahl-Chips ebenfalls die Uhrzeit). Ortszeit über die Zeitumstellung getestet.
> Uhrzeit ohne Datum = heute; Uhrzeit leeren = ganztägig. WebView-Selbsttest `p4due` (+ `p3new`) grün.
**Lies vorher:** `ui-guidelines.md` (Detailansicht), `features/task-detail/TaskDetail.svelte`,
`features/new-task/NewTaskDialog.svelte` (Datum + Uhrzeit, `tz`).

**Befund:** Die Detailansicht hat nur `<input type="date">` und setzt bei Änderung `{ kind: "date" }` – eine Task mit
Uhrzeit (`dateTime`, angelegt im Fenster „Neue Task") verliert beim Umstellen des Datums still ihre Uhrzeit; eine
Uhrzeit lässt sich nachträglich weder setzen noch ändern.

**Schritte:**
1. Uhrzeit-Feld neben dem Datum (wie im Fenster „Neue Task"; leer = ganztägig). Datum ändern behält die Uhrzeit,
   Uhrzeit leeren macht daraus `date`, Uhrzeit ohne Datum setzt heute.
2. Chips Heute/Morgen/In einer Woche behalten eine vorhandene Uhrzeit.
3. Umrechnung Datum + Uhrzeit ↔ `DueDto` in **eine** Hilfsfunktion (`lib/`), die Dialog und Detailansicht teilen.

**Fertig wenn:**
- Vitest: Datum einer `dateTime`-Task ändern → Uhrzeit bleibt; Uhrzeit leeren → `date`; Chip „Morgen" behält Uhrzeit.
- Selbsttest: Task mit `14:00` anlegen, im Detail Datum ändern, Uhrzeit nach Neuladen noch da.

### [x] P4-02 · „Heute": Abschnitt „Überfällig" + „Alle auf heute" — 2026-10-19 · c86ff06
> Notiz: **Abweichung:** Ein reiner UI-Schnitt reichte nicht – der Core sortierte „Heute"/„Geplant" nach `sort_key`,
> Abschnitte wären beim seitenweisen Laden zerfallen. Deshalb **ADR-026**: beide Ansichten sortieren im Core nach
> Fälligkeit (ungefährer lokaler Tagesbeginn ohne Zonendaten, UI sortiert geladene Seiten exakt nach), neue Cursor-Art,
> Contract-Fall `due_views_sorted_by_due`. Neuer Sammel-Aufruf `set_dues` (Core/Tauri/CoreClient) für „Alle auf heute"
> (ein Aufruf, Uhrzeit je Task bleibt, Rückgängig mit alten Werten). Gruppierung in `lib/groups.ts` + `GroupHeading`;
> die Einteilung für „Geplant" ist damit schon drin (Tests/Messung in P4-03). Zeilen unter „Heute" zeigen nur die
> Uhrzeit. Zähler der Seitenleiste: Überfällige rot aus denselben Seiten (kein zweiter Aufruf). Beim Kontrast-Test
> gefunden: `--color-danger` (hell) hatte auf Hover/Auswahl nur 4,1:1 → `#c42340` (≥ 4,8:1). Tests: Gruppen (7),
> Ansicht (4), Fake (1), Core-Use-Case (2), API, Contract; WebView-Selbsttest `p4today` grün.
**Lies vorher:** `ui-guidelines.md`, `features/task-list/TaskList.svelte`, `task-list-state.svelte.ts`,
`lib/format-date.ts` (`dueLabel`, Ton `overdue`), `features/sidebar/Sidebar.svelte` (`countFor`).

**Befund:** `SmartView::Today` liefert heute **und** überfällig in einer Liste; Überfälliges erkennt man nur am roten
Datum.

**Schritte:**
1. In „Heute" zwei Abschnitte: **Überfällig** (oben, nach Fälligkeit, älteste zuerst) und **Heute**. Trennung im UI
   über `due < heute` bzw. `dateTime` in der Vergangenheit (gleiche Regel wie `dueLabel`); kein neuer Core-Filter nötig.
   Überschrift mit Anzahl, z. B. „Überfällig · 3".
2. Am Abschnitt „Überfällig" die Aktion **„Alle auf heute"**: ein Aufruf für alle (Uhrzeit bleibt), Toast
   „3 Tasks auf heute verschoben" mit Rückgängig. Gibt es keinen Mehrfach-Aufruf für Fälligkeiten, in diesem Schritt
   `updateTasks`/`setDueMany` als **einen** Commit in Core + `CoreClient` ergänzen (wird in P4-04 wiederverwendet).
3. Abhaken/Verschieben aktualisiert die Abschnitte ohne Neuladen der Seite; der Fokus springt nicht in die Überschrift.
4. Seitenleiste: „Heute" zeigt die Anzahl überfälliger zusätzlich in `--color-danger` (aus derselben Abfrage wie der
   Zähler, kein zweiter Aufruf; mit FIX-04 dann über `count_tasks`).

**Fertig wenn:**
- Vitest: Aufteilung (Datum gestern, heute; `dateTime` heute vorbei/später), „Alle auf heute" + Rückgängig,
  ↑/↓ überspringt Überschriften, Zähler.
- Core-Test für den Mehrfach-Aufruf (ein Commit, Ops im Outbox), falls neu.
- Selbsttest in der WebView: drei überfällige Tasks → „Alle auf heute" → Abschnitt verschwindet.

### [x] P4-03 · „Geplant" nach Tagen gruppieren — 2026-10-19 · ed5ea7e
> Notiz: Einteilung kam schon mit P4-02 (`lib/groups.ts`); hier Tests, Messung, Feinschliff. Texte (`Intl`) nur noch
> je Überschrift statt je Task (sonst p95 28 ms beim Nachladen); flache Ansichten behalten ihr Eintrags-Objekt je Task.
> **Abweichung von Schritt 1:** Monate eines anderen Jahres tragen immer die Jahreszahl („Januar 2027"), nicht erst
> „über ein Jahr hinaus" – eindeutiger beim Jahreswechsel. Beim Messen gefunden: **FIX-12** (Listen luden nie eine
> zweite Seite, 2f3edcd). WebView `p4upcoming`: 1 000 geplante Tasks → 1 021 Einträge/21 Abschnitte geladen, Scrollen
> p50 17 / p95 18–24 ms (flache Liste 18–22), ↓ über Überschriften p95 17–18 ms; `p707` unverändert.
**Lies vorher:** wie P4-02, dazu `components/VirtualList.svelte`, `lib/local-day.ts`.

**Schritte:**
1. Überschriften je Tag: „Morgen", dann Wochentag + Datum für die nächsten 6 Tage („Mi, 21. Okt."), danach
   „Später in diesem Monat", Monatsnamen („November"), über ein Jahr hinaus mit Jahr. Lokale Zeitzone wie `local-day`.
2. Überschriften sind Einträge der virtuellen Liste (feste Höhe, wie Zeilen oder eigener Wert – `VirtualList` ggf. um
   Einträge unterschiedlicher Art erweitern, ohne pro Scroll alle Höhen neu zu rechnen).
3. Die Zeile zeigt in „Geplant" kein Datum mehr, wenn die Überschrift es schon sagt (nur Uhrzeit, falls vorhanden).
4. Gruppierung als reine Funktion in `lib/` (Tasks sortiert → Einträge), damit Board/Web sie wiederverwenden können.

**Fertig wenn:**
- Vitest: Gruppenbildung über Monats- und Jahresgrenze, Zeitumstellung, Tastatur-Navigation über Überschriften.
- Selbsttest: 1 000 geplante Tasks scrollen flüssig (Messung wie P0-Perf, Budget 16 ms pro Interaktion).

### [x] P4-04 · Fälligkeit per Kürzel und für mehrere Tasks — 2026-10-19 · 4476507
> Notiz: `features/due/` (`due-actions.ts`, `DuePrompt.svelte`); wirkt auf Board-Cursor, sonst Auswahl, sonst
> fokussierte Zeile; ein `set_dues`, Uhrzeit bleibt bei reinen Tagen. „Datum wählen…" nutzt `parseDue` – derselbe
> `scan()` wie `parseNewTask`, der eine reine Datumsangabe als Titel behält. **Abweichungen:** (1) Die Kürzel-Registry
> unterschied Shift bei Buchstaben nicht (`Umschalt+T` = `T`); jetzt schon – Folge: `Strg+Umschalt+A` ist nicht mehr
> `Strg+A` (Test angepasst, `ui-guidelines` §5). (2) Board-Karten haben kein Kontextmenü; dort gelten `T`/`D`, das
> Kontextmenü der **Zeile** und der Auswahl hat „Fälligkeit…". Toast nennt das Ziel („auf Fr, 23. Okt. verschoben"),
> nicht die behaltene Uhrzeit. Tests: 8 (Kürzel, Menü, Datum wählen, Auswahl ein Aufruf, nicht beim Tippen, Board,
> Kontextmenü) + `parseDue`; WebView `p4keys` grün.
**Lies vorher:** `lib/shortcuts.ts`, `App.svelte` (Registry, `SelectionBar`), `components/Menu.svelte`,
`features/new-task/parse.ts` (Datumswörter).

**Schritte:**
1. Fokussierte Task (Liste und Board): `T` = heute, `Umschalt+T` = morgen, `D` = Menü „Fälligkeit" (Heute, Morgen,
   Nächster Mo, In einer Woche, Datum wählen…, Entfernen). „Datum wählen…" nimmt Text wie die Titelzeile
   („fr", „24.10.", „morgen 14:00") über `parseNewTask`, damit kein zweiter Parser entsteht. Vorhandene Uhrzeit bleibt.
2. Auswahlleiste: neue Aktion „Fälligkeit…" (`D`) mit demselben Menü, ein Aufruf für alle (Mehrfach-Aufruf aus P4-02).
3. Kontextmenü der Zeile/Karte bekommt dieselben Einträge.
4. Toast nennt das Ergebnis („Auf Fr, 23. Okt. verschoben") und bietet Rückgängig.
5. Kürzel prüfen auf Konflikte mit bestehenden (`E`, `M`, `V`, `g …`) und in `ui-guidelines.md` eintragen.

**Fertig wenn:**
- Vitest: Kürzel für eine Task und für die Auswahl, Menü per Tastatur, Rückgängig, kein Auslösen beim Tippen in Feldern.
- Selbsttest: `T` auf einer Eingang-Task → erscheint in „Heute".

### [x] P4-05 · Suche und Befehlspalette — 2026-10-19 · 9717e77
> Notiz: ADR-027. Port-Methode `search_tasks` mit Standard-Implementierung (In-Memory) + SQLite FTS5-Trigram
> (Migration 0003; Update-Trigger nur bei Text-Änderung, sonst `move_tasks` 1 000 IDs 60 ms). Wörter < 3 Zeichen über
> `LIKE` (Groß/Klein nur ASCII). Messung Release, 10 000 Tasks: p95 ≤ 7,4 ms (Wort in allen), 0,2 ms (selten).
> Palette `features/palette/` lazy (eigener Chunk 3,3 KB gzip); Befehle über neues Feld `palette` in der Registry
> (Einträge ohne Taste nur in der Palette, nicht in der Kürzel-Übersicht). **Abweichungen:** (1) Es gibt noch keine
> Web-App – Budget am Desktop-Frontend gemessen: Start-Chunk 61,07 → 61,79 KB gzip (+0,7 KB Verdrahtung/Befehle, Ziel
> < 80 KB), Palette nicht im Start-Chunk. (2) Leerer Zustand zeigt nur zuletzt geöffnete **Tasks** + Befehle, keine
> „zuletzt besuchten Listen" (Listen stehen eh in der Seitenleiste). (3) Reine Notiztreffer zeigen „in Notiz" statt
> eines Ausschnitts (Summary ohne Notiztext). WebView `p4search`: Notiz-Teilwort „ERKLÄR" → richtige Task, Öffnen
> nach dem ersten Laden p95 39 ms (inkl. Frame).
**Lies vorher:** `architecture.md` (Performance, Web Worker), `interfaces.md` (`TaskRepository`, `CoreApi`),
`module-structure.md`, ADR-022/024; `crates/todo-storage-sqlite` (Schema, Migrationen).

**Ziel:** `Strg+K` (und `/` außerhalb von Feldern) öffnet eine Palette: Tippen zeigt passende Tasks aus allen Listen
(offene zuerst, erledigte gedämpft), dazu Listen und Befehle („Gehe zu Heute", „Neue Liste", „Einstellungen",
„Darstellung umschalten"). `Enter` öffnet die Task im Detail bzw. führt den Befehl aus.

**Schritte:**
1. **ADR-027** (Suche): Abfrage im Core als Port-Methode, z. B. `TaskRepository::search(text, limit)` bzw. ein Feld
   `text` in `TaskQuery`; durchsucht Titel und Notizen, ohne Groß/Klein und Umlaut-Probleme. SQLite: **FTS5**
   (Trigram-Tokenizer für Teilwörter) mit Trigger-Pflege und Migration – gegenüber `LIKE` begründen (keine O(n)-Arbeit
   pro Tastendruck, §4). Web/WASM-Repository mit bedenken (dort ggf. einfacher Index), Größe der Zusatzabhängigkeiten.
2. Core + Contract-Test (`testing/repository_contract.rs`): Treffer in Titel/Notiz, Umlaute, Teilwort, Limit, gelöschte
   Tasks nie. `CoreApi`/DTO + `just types`, Tauri-Command (`async`), `CoreClient` (IPC + WASM).
3. UI: `features/palette/CommandPalette.svelte`, **lazy geladen** (§4); Eingabe entprellt (~100 ms), höchstens
   50 Treffer, Treffer-Teil hervorgehoben; Abschnitte „Tasks", „Listen", „Befehle". Befehle kommen aus der
   Kürzel-Registry (eine Quelle für Palette, `ShortcutHelp` und Kürzel).
4. Leerer Zustand: zuletzt geöffnete Tasks/Listen (nur im Speicher der Sitzung). Keine Treffer: „Keine Task mit
   ‚…' – mit Enter als neue Task anlegen" (öffnet „Neue Task" mit dem Text).

**Fertig wenn:**
- ADR-027 in `tech-decisions.md`; Core-, Contract- und Vitest-Tests grün.
- Messung: Suche über 10 000 Tasks < 16 ms im Core (Release), Palette öffnet < 50 ms nach dem ersten Laden.
- Initiales JS-Budget der Web-App unverändert (Palette lazy).
- Selbsttest: `Strg+K`, Teilwort einer Notiz tippen, `Enter` öffnet die richtige Task.

### [x] P4-06 · „Erledigt" nach Tagen gruppieren + aufräumen — 2026-10-19 · 39ad547
> Notiz: Gruppen „Heute", „Gestern", übrige Tage dieser Woche („Dienstag"), „Letzte Woche", Monate (fremdes Jahr mit
> Jahreszahl) aus `lib/groups.ts`. Aufräumen im neuen „⋯"-Menü der Ansicht „Erledigt": Stichtag = Beginn des Tages
> vor 30 Tagen, Bestätigung nennt Anzahl und Datum, Löschen über `deleteManyWithUndo`; ohne Treffer nur ein Hinweis.
> Tests: Gruppen (2), Ansicht + Aufräumen + Rückgängig (3).
**Lies vorher:** P4-03 (Gruppierung), `lib/task-actions.ts` (`deleteManyWithUndo`).

**Schritte:**
1. Abschnitte nach Erledigt-Datum: „Heute", „Gestern", Wochentage dieser Woche, „Letzte Woche", Monatsnamen
   (Gruppierungsfunktion aus P4-03 wiederverwenden, Schlüssel = `completedAt`).
2. Listenaktion „Erledigte älter als 30 Tage löschen" (Kopfzeilen-Menü der Ansicht): Bestätigung mit Anzahl, Löschen
   über `deleteManyWithUndo` (je 2 000, Rückgängig).

**Fertig wenn:**
- Vitest: Gruppen, Aufräumen nur > 30 Tage, Rückgängig stellt alle wieder her.

### [~] P4-07 · Schnelleingabe direkt in der Liste — wartet auf den User
Vorschlag aus dem UX-Review: eine Eingabezeile oben in der Liste mit demselben Parser („Bericht fr 14:00 !1 @Uni"),
`Enter` legt an und bleibt für die nächste; das Fenster „Neue Task" bleibt für Notizen/Details.
**Konflikt:** P3-02 hat genau diese Zeile auf Wunsch des Users entfernt („mehr Platz"), und „Weitere anlegen" im
Fenster deckt schnelles Erfassen teilweise ab. Nur aufnehmen, wenn der User das ausdrücklich will – sonst streichen.
Mögliche Variante ohne Platzverlust: Zeile erscheint nur nach `N` bzw. am Ende der Liste statt dauerhaft oben.

---

## Bugs / Nachträge

_(Neue Aufgaben mit ID `FIX-nn` hier anhängen. Offene Punkte aus dem Prototyp stehen auch in `roadmap-next.md` N0;
erledigte FIX-Einträge mit Notizen im Archiv. Nächste freie Nummer: FIX-13.)_

- [-] FIX-01 · CI-Workflow (`just check`) als GitHub Actions — **verworfen 2026-10-18 (User: „Wir wollen keine CI
  Pipeline")**. Prüfung bleibt lokal: `just check` + Pre-commit-Hook (`.githooks`).
  > Notiz: Ein Workflow war schon gebaut und im Container getestet, wurde aber vor dem Push wieder aus der
  > Historie entfernt.
- [x] FIX-03 · `CreateTask`/`MoveTask` laden die Zielliste komplett (O(n), ~7 ms bei 10 000 Tasks). Repository-Methode
  für letzten Sort-Key bzw. Nachbar-Keys ergänzen, wenn Listen dieser Größe realistisch werden. _(→ N0-03; betrifft
  auch `MoveTasks` aus P0-01 – dort pro Zielliste nur **einmal** laden.)_ — 2026-10-18 · 3ffd69d
  > Notiz: Anhängen über `TaskRepository::last_sort_key` (SQLite: ein Zugriff auf `tasks_by_list`; Default über
  > `list_tasks`, Contract-Test `last_sort_key_of_list`): `create_task` bei 10 000 Tasks 6,8 → 0,13 ms; auch
  > `DeleteTaskList` (Tasks → Inbox) nutzt es. `MoveTasks` lädt pro Aufruf einmal. Einfügen zwischen Nachbarn bleibt
  > O(n) → FIX-11 (ADR-024).
- [ ] FIX-04 · Sidebar-Zähler (Eingang/Heute) zählen über `listTasks`-Seiten (bis 999); `count_tasks(query)` in
  Repository/CoreApi spart IPC. _(→ N0-03)_
- [ ] FIX-06 · Live-Teil der Zwei-Geräte-Abnahme über Proton Drive + Messung angemeldet. _(→ N0-01)_
- [-] FIX-07 · (zurückgestellt 2026-10-18, User: RAM ok) RAM-Ziel < 250 MB PSS. _(→ N0-02)_
- [x] FIX-08 · **Fehlgeschlagenes Löschen lässt die Zeile verschwinden.** `TaskList.deleteTask` entfernt die Zeile per
  `rows.remove()` *bevor* `deleteWithUndo` löscht; schlägt `getTask`/`deleteTask` fehl, kommt nur ein Fehler-Toast –
  die Task bleibt bis zum Ansichtswechsel unsichtbar, obwohl sie noch existiert. `TaskListState.hide()`/`unhide()`
  sind dafür gedacht, werden aber nirgends benutzt (toter Code; `hidden` ist immer leer). Fix: `hide()` vor dem
  Löschen, bei Fehler `unhide()`. _(gefunden beim Planen von P0; erledigt in P0-05, 11a4bd6)_
- [x] FIX-09 · **Rückgängig nach Löschen ist nicht atomar und verliert Felder.** `restore()` in `lib/task-actions.ts`
  braucht 3–4 einzelne Commits (`createTask`, `updateTask`, ggf. `completeTask`, `moveTask`): bricht es mittendrin
  ab (Fehler, App geschlossen), bleibt eine halbe Kopie (nur Titel) zurück; außerdem gehen `createdAt` und
  `completedAt` verloren (Kopie bekommt „jetzt"), und es entstehen unnötig viele Ops im Outbox. Fix: `RestoreTasks`
  aus P0-01 (ein Commit, alle Felder, alter Sort-Key). _(gefunden beim Planen von P0; erledigt in P0-01/P0-05, 8a1ee9d/11a4bd6)_
- [x] FIX-10 · **Parallele `create_task` vergeben gleiche Sort-Keys.** `CreateTask` liest die Zielliste und committet
  getrennt; laufen mehrere Aufrufe gleichzeitig (Selbsttest legt je 50 per `Promise.all` an), bekommen sie denselben
  Key – 500 Tasks hatten nur 42 verschiedene Keys. Reihenfolge ist dann die der IDs (Tiebreak), nicht die des Anlegens,
  und Einfügen zwischen solchen Nachbarn braucht den FIX-02-Sonderfall. Fix: Key-Vergabe innerhalb der Transaktion
  (Repository-Methode „anhängen") oder Use-Case serialisieren; zusammen mit FIX-03. _(gefunden in P0-05, 2026-10-18)_
  — 2026-10-18 · 3ffd69d
  > Notiz: Use-Case serialisiert (ADR-024): Reihenfolge-Sperre in `Deps` für `CreateTask`, `CreateTaskList`,
  > `MoveTask(s)`, `MoveTaskList`, `DeleteTaskList`. Test `parallel_creates_get_distinct_sort_keys` (50 parallele
  > `create_task` über SQLite) – ohne Sperre rot (1 Key), mit Sperre 50 verschiedene.
- [ ] FIX-11 · `MoveTask`/`MoveTasks` mit Nachbarn laden die Zielliste komplett (O(n), 6,9 ms bei 10 000 Tasks, im
  16-ms-Budget). Repository-Abfrage für Nachbar-Keys (inkl. Lauf gleicher Keys für den FIX-02-Sonderfall), sobald
  Listen > 5 000 Tasks realistisch werden. _(Rest aus FIX-03, 2026-10-18)_
- [x] FIX-12 · **Listen laden beim Scrollen keine weiteren Seiten.** `TaskListState.cursor` war ein normales Feld;
  `onEndReached={rows.hasMore ? … : undefined}` in `TaskList` wurde einmal vor der ersten Seite ausgewertet (Cursor
  `null`) und nie wieder – Ansichten mit > 200 Tasks zeigten nur die erste Seite (fiel in `p707` nicht auf, weil dort
  die Tasks per Änderungs-Event hereinkommen). Fix: Cursor als `$state`. _(gefunden in P4-03, 2026-10-19)_
  — 2026-10-19 · 2f3edcd
