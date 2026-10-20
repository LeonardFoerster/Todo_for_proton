# Archiv: Zwischenphasen P0–P3 (abgeschlossen 2026-10-18)

Aus `roadmap.md` hierher verschoben (2026-10-18). Achtung: Die IDs P0–P2 überschneiden sich mit den Prototyp-Phasen
in `roadmap-prototype.md`. Verweise auf Sammel-Use-Cases, Mehrfachauswahl, Upload-Debounce (ADR-021/022) oder
Listen/Kanban (ADR-023) meinen **diese** Datei; Verweise auf Proton-Probe, SRP/Fork-Login oder Drive-Live-Proben meinen
`roadmap-prototype.md`.

---

## P0 · Mehrfachauswahl & Sammelaktionen

**Ziel:** Über einen Button „Auswählen" in der Kopfzeile kommt man in einen Auswahlmodus. Dort wählt ein Klick eine
Task aus oder ab, `Umschalt`+Klick wählt einen Bereich, `Strg`+Klick schaltet einzelne Tasks um, „Alle auswählen"
nimmt die ganze Ansicht. Die Auswahl gilt für **Erledigt/Wieder öffnen, Löschen, Verschieben in eine Liste und
Umsortieren** – jeweils mit einem Rückgängig-Toast für die ganze Auswahl.

**Festlegungen (für alle Aufgaben der Phase):**

- **Cursor ≠ Auswahl.** `selectedId` in `App.svelte`/`TaskList.svelte` bleibt der *Cursor* (fokussierte Zeile,
  Tastaturnavigation, Detail-Panel). Neu ist eine *Auswahl* (Menge von IDs + Anker für Bereiche). Ohne Auswahlmodus
  verhält sich alles wie bisher.
- **Auswahlmodus an:** Button „Auswählen" (Kopfzeile, Icon `ListChecks`), `Strg`/`Umschalt`+Klick auf eine Zeile
  (startet den Modus mit Cursor-Zeile bzw. Bereich – wie im Dateimanager) oder `Strg+A` in der Liste.
  **Aus:** „Fertig" in der Aktionsleiste, `Esc` oder Ansichtswechsel (TaskList ist per `{#key}` je Ansicht neu).
  Wird die Auswahl durch eine Aktion leer, bleibt der Modus an.
- **Im Modus:** Zeilen zeigen links ein eckiges Auswahlkästchen (unterscheidbar vom runden Erledigt-Kästchen, das
  bleibt und wirkt weiter nur auf die eine Zeile). Klick auf die Zeile = umschalten (öffnet **nicht** das Detail),
  `Umschalt`+Klick = Bereich Anker → Zeile hinzufügen, `Strg`+Klick = umschalten und Anker setzen. Detail-Panel wird
  beim Eintritt geschlossen.
- **Tastatur im Modus:** `↑/↓`, `J/K` bewegen den Cursor; `Umschalt+↑/↓` erweitert die Auswahl; `X` bzw. `Leertaste`
  schaltet die Cursor-Zeile um; `Strg+A` wählt alle (sind schon alle ausgewählt: keine); `E` erledigt/öffnet alle ausgewählten;
  `Entf` löscht alle ausgewählten; `M` öffnet „Verschieben nach…"; `Alt+↑/↓` verschiebt den Auswahlblock
  (nur in sortierbaren Ansichten); `Esc` beendet den Modus. In Textfeldern bleibt `Strg+A` nativ.
- **Aktionsleiste** (ersetzt im Modus die Kopfzeilen-Aktionen, keine zweite Zeile, kein Layout-Sprung):
  Tri-State-Kästchen „Alle auswählen" · „3 ausgewählt" · Erledigt · Wieder öffnen · Verschieben nach… (Menü mit
  Listen) · Löschen · Fertig. Aktionen sind bei leerer Auswahl deaktiviert; „Erledigt"/„Wieder öffnen" je nachdem,
  ob offene bzw. erledigte Tasks in der Auswahl sind.
- **Alle auswählen** meint die ganze Ansicht, nicht nur geladene Seiten: fehlende Seiten werden nachgeladen
  (`TaskListState.loadAll()`), bevor ausgewählt wird. In „Erledigt" und Sammelansichten genauso.
- **Kontextmenü** auf einer ausgewählten Zeile wirkt auf die ganze Auswahl („3 Tasks löschen"), auf einer nicht
  ausgewählten Zeile wie bisher nur auf diese.
- **Grob-granulare Grenze** (AGENTS.md §4): Sammelaktionen sind **ein** Core-Aufruf mit allen IDs, ein Commit
  (eine Transaktion, Ops im Outbox) und **ein** `Changed`-Event – keine Schleifen über Einzel-Commands in der UI.
- **Unbekannte IDs** in Sammelaufrufen (z. B. inzwischen per Sync gelöscht) werden übersprungen, nicht als Fehler
  gemeldet; das Ergebnis enthält nur die tatsächlich geänderten Tasks. Andere Fehler: ganzer Aufruf schlägt fehl,
  nichts ist geschrieben.
- **Keine O(n)-Arbeit pro Klick/Taste:** Auswahl als `SvelteSet`, Zeile bekommt `selected` per `has()`; Bereich
  kostet O(Bereichsgröße); nur „Alle auswählen" ist O(n).
- **Barrierefreiheit:** `aria-multiselectable="true"` auf der Listbox im Modus, `aria-selected` = in der Auswahl,
  Live-Region meldet die Anzahl („3 ausgewählt").

### [x] P0-01 · Core: Sammel-Use-Cases — 2026-10-18 · 8a1ee9d
**Lies vorher:** `interfaces.md` §1–3, `sync-strategy.md` (Ops, Tombstones), `crates/todo-core/src/app/` (vor allem
`move_task.rs`, `delete_task.rs`, `complete_task.rs`), ADR-011 (Sort-Keys).
**Schritte:**
1. Use-Cases in `crates/todo-core/src/app/` (je eine Datei, Muster wie die Einzel-Use-Cases), alle mit **einem**
   `ChangeSet`:
   - `CompleteTasks { ids, done }` → geänderte Tasks (bereits im Zielzustand: übersprungen, kein Op).
   - `DeleteTasks { ids }` → die Tasks **vor** dem Löschen (Grundlage für Rückgängig, spart `getTask` je Task).
   - `MoveTasks { ids, list_id, after, before }` → verschiebt die Tasks als Block in ihrer bisherigen
     Anzeigereihenfolge an die Zielposition (Semantik der Nachbarn wie `MoveTaskInput`; ohne Nachbarn ans Ende der
     Zielliste). Sort-Keys: n Keys nacheinander per `SortKey::between` zwischen den Nachbarn erzeugen; der
     Sonderfall gleicher Nachbar-Keys (FIX-02) wie in `MoveTask`. Gemischte Quelllisten erlaubt (Sammelansichten).
   - `RestoreTasks { tasks }` → legt Kopien gelöschter Tasks mit **neuen** IDs an (eine gelöschte ID lässt sich im
     Sync-Modell nicht wiederbeleben), mit allen Feldern: Titel, Notizen, Priorität, Fälligkeit, Status,
     `completed_at`, `created_at`, Liste und **altem Sort-Key** (Position bleibt ohne Nachbar-Suche erhalten;
     Gleichstand löst die `TaskId`-Regel). Existiert die Liste nicht mehr → Inbox.
2. Validierung: leere ID-Liste = kein Commit, leeres Ergebnis; doppelte IDs entfernen; Obergrenze
   `BULK_LIMIT_MAX` (z. B. 2 000) → `Validation`.
3. `CoreApi`: `complete_tasks`, `delete_tasks`, `move_tasks`, `restore_tasks` mit DTOs (`Vec<String>`,
   `MoveTargetDto`, `Vec<TaskDto>`); ein `Changed`-Event mit allen IDs.
4. Tests: je Use-Case Unit-Tests (In-Memory-Repo): Erfolg, übersprungene unbekannte IDs, Leer-/Duplikatfall,
   Block-Reihenfolge beim Verschieben (Anfang/Mitte/Ende, andere Liste, gemischte Quelllisten, gleiche Nachbar-Keys),
   Restore mit gelöschter Liste. Ein Test „ein Commit": genau ein `Changed`-Event, Outbox-Ops = geänderte Felder.
   Mehrgeräte-Test in `todo-sync` (Szenario wie P5-09): Gerät A verschiebt einen Block, Gerät B ändert parallel eine
   Task des Blocks → Konvergenz.
5. SQLite-Messung (wie P4-04): `delete_tasks`/`move_tasks` mit 1 000 IDs in einer 10 000er-DB, Zeit notieren.
6. ADR-021 „Sammel-Operationen in der Core-API" (ein Commit pro Aktion, Überspringen unbekannter IDs, Restore mit
   neuen IDs und altem Sort-Key).

**Fertig wenn:** `just check` grün, Messung < 50 ms für 1 000 IDs (sonst Notiz + FIX), ADR-021 eingetragen,
`interfaces.md` um die neuen Methoden ergänzt. Commit `feat(core): bulk task operations`.
> Notiz: Sort-Keys nicht „nacheinander", sondern per neuem `SortKey::n_between` (Halbierung; nacheinander wüchsen
> die Keys zwischen zwei festen Nachbarn linear, ~170 Zeichen bei 1 000). `MoveTasks` ohne `list_id` nur, wenn alle
> Tasks in einer Liste liegen (sonst `Validation`). Messung zuerst 68–103 ms → Ursache: Statement-Journal des
> Fremdschlüssels als Temp-Datei; behoben mit `temp_store = MEMORY`, dazu `TaskRepository::get_tasks` und
> `set_entity_state` nur für geänderte Register (ADR-021 Punkt 5). Jetzt move 28 / complete 21 / delete 17 /
> restore 25 ms bei 1 000 IDs in 10 000 Tasks (`architecture.md` §6). Als Nebeneffekt: 10 000 Tasks einfügen 645 →
> 238 ms. `restore_tasks` setzt Tags/Unteraufgaben leer (`TaskDto` kennt sie noch nicht).

### [x] P0-02 · Shell & `CoreClient`: Sammel-Commands — 2026-10-18 · 4989a25
**Lies vorher:** ADR-019, `apps/desktop/src-tauri/src/commands.rs`, `packages/core-client/src/core-client.ts`.
**Schritte:**
1. Tauri-Commands `complete_tasks`, `delete_tasks`, `move_tasks`, `restore_tasks` (dünn, `async`, je **ein**
   `state.sync.local_change()`), in `lib.rs` registrieren.
2. `CoreClient`: `completeTasks(ids, done)`, `deleteTasks(ids)`, `moveTasks(ids, target)`, `restoreTasks(tasks)`;
   Implementierungen in `tauri-core-client.ts` und `fake-core-client.ts` (Fake: gleiche Semantik inkl. Überspringen
   und einem `onChange`), `just types`.
3. Vitest für den Fake (Reihenfolge beim Block-Verschieben, ein Event pro Aufruf).
4. `todo-wasm` hat noch keine Bindings – nichts zu tun; in N1-03 mit aufnehmen (Notiz dort).

**Fertig wenn:** `just check` grün, ein Dev-Lauf ruft jeden Command einmal erfolgreich auf (Selbsttest oder
`just desktop-dev` + Konsole). Commit `feat(desktop): bulk task commands`.
> Notiz: Nachweis doppelt – IPC-Test über MockRuntime (`src-tauri/tests/commands.rs`, `bulk_commands_through_ipc`)
> und Selbsttest-Suite `p002` in der echten WebView (`TASKS_SELFTEST=p002`: alle vier Commands, je ein Event,
> Reihenfolge stimmt). Commands zusätzlich in `capabilities/default.json`. N1-03 nennt die Sammel-Methoden bereits.

### [x] P0-03 · UI-Zustand: `SelectionState` — 2026-10-18 · 693c381
**Lies vorher:** `packages/ui/src/features/task-list/task-list-state.svelte.ts`, `ui-guidelines.md` §5 und §7.
**Schritte:**
1. `packages/ui/src/features/task-list/selection.svelte.ts`: `active`, `ids` (`SvelteSet`), `anchor`, `count`;
   `enter()`, `exit()`, `toggle(id)`, `range(toId, order)` (Anker → Ziel über die aktuelle Reihenfolge,
   **hinzufügen**), `extend(delta)` für `Umschalt+↑/↓`, `selectAll(ids)`, `clear()`, `prune(present)`
   (entfernt IDs, die aus der Ansicht verschwunden sind – nach Sync, Löschen, Verschieben in andere Ansicht).
2. `TaskListState.loadAll()`: restliche Seiten nachladen (für „Alle auswählen"); Liste der IDs liefern.
3. Abgeleitete Werte für die Aktionsleiste: `hasOpen`, `hasDone` (über die ausgewählten, geladenen Summaries).
4. Vitest: Umschalten, Bereich vorwärts/rückwärts, Bereich mit fehlendem Anker (= Cursor-Zeile), `extend` an den
   Rändern, `prune`, `selectAll` mit nachgeladenen Seiten (Fake-Client mit > 200 Tasks).

**Fertig wenn:** `just ts` grün, keine UI-Änderung sichtbar. Commit `feat(ui): selection state`.
> Notiz: `hasOpen`/`hasDone` als `SelectionState.flags(lookup)` (läuft über die Auswahl, nicht über alle Zeilen; bricht
> ab, sobald beides feststeht); dafür `TaskListState.byId` mit Index je `items`-Array. `extend` fügt nur hinzu (kein
> Verkleinern beim Zurücklaufen). `loadAll` lädt mit Seitengröße 500 (Core-Maximum) statt 200.

### [x] P0-04 · UI: Auswahlmodus, Klick- und Tastaturauswahl — 2026-10-18 · 7aecdc5
**Lies vorher:** `TaskList.svelte`, `TaskRow.svelte`, `App.svelte` (Kürzel), `lib/shortcuts.ts`, Festlegungen oben.
**Schritte:**
1. Button „Auswählen" in der Kopfzeile (`App.svelte`, `IconButton` mit Tooltip + Kürzel); im Modus Aktionsleiste
   statt Kopfzeilen-Aktionen (zunächst nur Tri-State „Alle auswählen", Zähler, „Fertig"; Aktionen folgen in P0-05).
2. `TaskRow`: Prop `selectable` + `checked`; eckiges Auswahlkästchen links (neue kleine Variante von `Checkbox` oder
   eigenes Element, Tokens wie Fokus/Akzent), Zeile im Modus mit Auswahlfarbe `--color-bg-selected`; der Cursor
   behält den Akzentbalken links.
3. Klick-Logik in `TaskList`: Modus aus → Klick öffnet wie bisher, `Strg`/`Umschalt`+Klick startet den Modus
   (Cursor-Zeile bzw. Bereich ab Cursor). Modus an → Klick/`Strg`+Klick umschalten, `Umschalt`+Klick Bereich.
   `user-select: none` ist schon gesetzt – Umschalt+Klick markiert keinen Text.
4. Kürzel registrieren (`when`: Modus aktiv bzw. Liste fokussiert): `x`, `space` (im Modus statt Erledigt),
   `shift+arrowup/down`, `ctrl+a` (alle ↔ keine), `escape` (Modus beenden, vor „Auswahl aufheben" in `escape()`).
   Hinweis: `comboOf` verwirft Shift bei Zeichentasten – `Strg+Umschalt+A` wäre nicht von `Strg+A` zu
   unterscheiden, deshalb bewusst kein eigenes „Keine auswählen"-Kürzel.
5. Detail-Panel beim Eintritt schließen; `prune` an `rows.items` koppeln (Effekt nur über die Menge, nicht pro Zeile).
6. ARIA: `aria-multiselectable`, `aria-selected`, Live-Region mit Zähler.
7. Vitest für die Kürzel-Kombinationen; Kürzel-Übersicht (`ShortcutHelp`) zeigt eine Gruppe „Auswahl".

**Fertig wenn:** `just check` grün; im Dev-Selbsttest (P0-07) bzw. von Hand: Modus an/aus per Button und `Esc`,
`Strg`/`Umschalt`+Klick, `Umschalt+↑/↓`, `Strg+A` mit > 200 Tasks wählt alle. Commit `feat(ui): multi-select mode`.
> Notiz: `SelectionState` gehört der `TaskList` (endet so mit der Ansicht), `App` liest sie über die Instanz.
> Auswahlkästchen als neue Komponente `SelectBox` (eckig, Tri-State für „Alle auswählen"; in Zeilen dekorativ, die
> Zeile trägt `aria-selected`). `Umschalt+↑/↓` startet den Modus auch von außen (wie `Umschalt`+Klick). `Enter`,
> `Entf`, `E`/`Leertaste` als Einzelaktionen sind im Modus gesperrt (Sammelvarianten folgen in P0-05). Button-Tooltip
> zeigt „Strg+A". Nachweis in der echten WebView: Selbsttest-Suite `p0sel` (250 Tasks; `Strg+A` inkl. Nachladen
> 26 ms), dazu `multi-select.test.ts`.

### [x] P0-05 · UI: Sammelaktionen mit Rückgängig — 2026-10-18 · 11a4bd6
**Lies vorher:** `lib/task-actions.ts`, `components/Menu.svelte`, `TaskDetail.svelte` (`moveTo`), FIX-08, FIX-09.
**Schritte:**
1. Aktionsleiste: Erledigt, Wieder öffnen, Verschieben nach… (Menü mit allen Listen, Inbox zuerst; aktuelle Liste
   in einer Listenansicht deaktiviert), Löschen. Kürzel `e`, `delete`, `m` im Modus.
2. `lib/task-actions.ts`: `deleteManyWithUndo(client, ids)` → `deleteTasks`, Toast „3 Tasks gelöscht ·
   Rückgängig" → `restoreTasks(snapshots)`; `completeManyWithUndo(client, ids, done)` → Toast mit Rückgängig (stellt
   nur die tatsächlich geänderten zurück); `moveManyWithUndo` → Rückgängig verschiebt jede Task zurück in ihre Liste
   **an ihren alten Sort-Key**-Platz (Ziel pro Quellliste als ein `moveTasks`-Aufruf je Liste, Reihenfolge
   erhalten). Einzel-Löschen nutzt dieselben Wege mit einer ID (ersetzt `getTask`+`deleteTask` und das
   mehrstufige `restore` → behebt FIX-09).
3. Optimistisch: ausgewählte Zeilen per `rows.hide()` ausblenden bzw. Status setzen; bei Fehler `unhide`/zurücksetzen
   und Fehler-Toast (behebt FIX-08 für Einzel- und Sammel-Löschen). Nach Löschen/Verschieben aus der Ansicht wird die
   Auswahl geleert, der Modus bleibt; Cursor auf die nächste verbliebene Zeile.
4. Kontextmenü: auf ausgewählter Zeile Sammelvarianten („3 Tasks erledigen", „Verschieben nach…", „3 Tasks
   löschen"), sonst Einzelaktionen wie bisher.
5. Vitest mit dem Fake: je Aktion ein Core-Aufruf, Rückgängig stellt Zustand/Reihenfolge wieder her, Fehler setzt
   die Zeilen zurück.

**Fertig wenn:** `just check` grün; 500 ausgewählte Tasks löschen → ein IPC-Aufruf, Liste sofort leer,
Rückgängig bringt alle an alter Stelle zurück. Commit `feat(ui): bulk actions with undo`.
> Notiz: Rückgängig nach Verschieben lädt je Quellliste die übrigen Tasks und setzt jede Task vor den ersten übrigen
> mit größerem (Sort-Key, ID); Tasks zwischen denselben Nachbarn gehen in einem `moveTasks` – zusammenhängende Auswahl =
> ein Aufruf je Liste, verstreute Auswahl einer je Lücke (sonst wäre die alte Stelle nicht exakt). Große
> `Changed`-Events (> 20 IDs) laden die geladenen Seiten neu statt je ID `getTask` (sonst 500 IPC-Aufrufe nach einer
> Sammelaktion). Befund beim WebView-Test: `RestoreTasks` vergibt neue IDs jetzt in alter Reihenfolge (gleiche Sort-Keys
> blieben sonst nicht stabil, ADR-021 Punkt 3); Ursache der gleichen Keys → FIX-10. Nachweis WebView (Suite `p0sel`):
> 500 löschen = 1 Aufruf, Liste nach 21 ms leer, 0 `getTask`, Rückgängig stellt die exakte Reihenfolge her; Erledigt
> und Verschieben je mit Rückgängig. FIX-08 und FIX-09 damit erledigt. Vitest-Timeout in `packages/ui` auf 15 s
> (unter Volllast fielen sonst auch bestehende Tests).

### [x] P0-06 · UI: Auswahlblock umsortieren — 2026-10-18 · 5b618b1
**Lies vorher:** `TaskList.svelte` (`moveTo`, `moveSelected`, `startDrag`), `TaskListState.reorder`.
**Schritte:**
1. `TaskListState.reorderMany(ids, to)`: Block (in Anzeigereihenfolge, auch nicht zusammenhängende Auswahl) an
   Einfügeposition schieben, liefert die Nachbarn für `moveTasks`.
2. `Alt+↑/↓` und Kontextmenü „Nach oben/unten" verschieben im Modus den ganzen Block um eine Position (nicht
   zusammenhängende Auswahl wird dabei zusammengezogen – wie in gängigen Listen-Apps; als Notiz dokumentieren).
3. Ziehen einer ausgewählten Zeile zieht den Block (Zähler-Badge „3" am Zeiger, Drop-Linie wie bisher); Ziehen
   einer nicht ausgewählten Zeile bleibt Einzel-Verschieben.
4. Nur in sortierbaren Ansichten (`inbox`/`list`); sonst deaktiviert mit Tooltip.
5. Vitest für `reorderMany` (zusammenhängend, verstreut, an Anfang/Ende, Block über sich selbst).

**Fertig wenn:** `just check` grün; Block per Tastatur und Maus verschiebbar, Reihenfolge nach Neustart und nach
Sync auf dem zweiten Profil gleich. Commit `feat(ui): reorder selected tasks`.
> Notiz: Verstreute Auswahl wird beim Verschieben zusammengezogen: `Alt+↑` setzt den Block über die Zeile vor der ersten
> ausgewählten, `Alt+↓` unter die Zeile nach der letzten (`A B C D E`, Auswahl B+D, `Alt+↓` → `A C E B D`). Jedes
> Umsortieren ist ein `moveTasks` mit Rückgängig-Toast (wie die übrigen Sammelaktionen). Ziehen einer ausgewählten Zeile
> bei nur einer ausgewählten = Einzel-Verschieben. Einfügelinie entfällt, wenn ein zusammenhängender Block auf sich selbst
> fiele. Nachweis: Vitest (`reorderMany`, Tastatur, Ziehen, nicht sortierbare Ansicht), WebView-Selbsttest `p0sel`
> (Alt+↓ und Ziehen mit Badge „2"), Zwei-Profil-Lauf (Abschnitt 5 des Abnahmeskripts, mit P0-07 committet): B sieht
> dieselbe Reihenfolge, nach Neustart von A unverändert. Dabei gefunden und behoben: Nach Sammel-Löschen/-Verschieben
> blieb die Ansicht an der alten Scroll-Position, der Cursor war nicht sichtbar.

### [x] P0-07 · Selbsttest, Messung, Doku & Abnahme — 2026-10-18 · 0b976bc
**Lies vorher:** `apps/desktop/src/selftest.ts`, `tools/acceptance-two-devices.sh`, `ui-guidelines.md` §4–§5.
**Schritte:**
1. Dev-Selbsttest um einen Abschnitt „Mehrfachauswahl" erweitern (in der echten WebView, keine globalen Eingaben):
   Modus per Button, Klick/`Strg`/`Umschalt`, `Strg+A` mit > 200 Tasks, Erledigt/Löschen/Verschieben je mit
   Rückgängig, Block-Umsortieren, `Esc`.
2. Messung: Repaint nach Klick bzw. `Umschalt+↓` mit 1 000 Tasks und 1 000 ausgewählten < 16 ms (Ziel) / < 50 ms
   (hart); „Alle auswählen" mit 10 000 Tasks notieren. Idle-CPU im Modus weiter ~0 %.
3. Zwei-Geräte-Lauf (`tools/acceptance-two-devices.sh`) um einen Sammel-Schritt erweitern (Gerät A löscht und
   verschiebt je 50 Tasks, Gerät B sieht dasselbe).
4. Doku: `ui-guidelines.md` §4 (Auswahlkästchen, Aktionsleiste) und §5 (neue Kürzel), `interfaces.md`
   (`CoreClient`), `architecture.md` falls Messwerte; `AGENTS.md` unverändert, außer der Status-Zeile.
5. Abnahme durch den User am Desktop (Checkliste aus den Festlegungen oben); Funde als FIX unten.

**Fertig wenn:** `just check` und Selbsttest grün, Budgets eingehalten, User-Abnahme eingetragen.
Commit `test(desktop): multi-select acceptance`. **Danach:** weiter mit `roadmap-next.md` N0-01.
> Notiz (2026-10-18 · 571caf3, 0b976bc): Selbsttest `p0sel` (Modus, Klick/Strg/Umschalt, `Umschalt+↓`, `Strg+A` über
> 500, Erledigt/Verschieben/Löschen je mit Rückgängig, Block per `Alt+↓` und Ziehen, `Esc`) grün in der echten WebView.
> Messung (`architecture.md` §6, Debug + Vite-Dev, also pessimistisch): Klick bzw. `Umschalt+↓` bis Repaint bei 1 000
> und 10 000 ausgewählten p95 17–19 ms (= nächster Frame; Ziel < 16, hart < 50 ✔), „Alle auswählen" mit 10 000 529 ms,
> Leerlauf im Modus 2,11 % vs. 2,03 % ohne Modus (Grundlast des Debug-Aufbaus, kein Mehrverbrauch ✔). Abnahmeskript
> Abschnitt 5 (je 50 löschen/verschieben + Block, B gleich, Neustart gleich) und Abschnitte 1–4 erneut grün. Doku:
> `ui-guidelines.md` §4/§5, `interfaces.md` (P0-02), `architecture.md`, `prototype-acceptance.md` (A5 + Checkliste C),
> AGENTS.md-Status. Dabei behoben: Auswahl > 2 000 scheiterte an `BULK_LIMIT_MAX` (jetzt Pakete), Bereich/`Umschalt+↓`
> bauten je Taste ein ID-Array (jetzt O(1)-Index). **User-Abnahme am Desktop: bestanden (2026-10-18)**, Checkliste
> `prototype-acceptance.md` Teil C, keine Funde. Phase P0 abgeschlossen → weiter mit `roadmap-next.md` N0-01.

---

## P1 · Erledigte verlassen offene Ansichten · schneller Upload

**Ziel:** (1) Abgehakte Tasks verschwinden nach kurzer Haken-Animation aus allen offenen Ansichten (Eingang, Listen,
Heute, Geplant, Alle) und sind nur noch unter „Erledigt" zu sehen; Rückgängig im Toast. (2) Upload nach jeder
definierten lokalen Änderung (Task anlegen/ändern/abhaken/verschieben/löschen, Sammel-Varianten, Listen anlegen/
umbenennen/löschen): einzeln nach ~1,5 s, bei Serien spätestens 5 s nach der ersten Änderung, ein Lauf je Serie.
Plan: `~/.claude/plans/ich-m-chte-dass-wir-idempotent-waterfall.md`.

### [x] P1-01 · Abgehakte Tasks verlassen offene Ansichten — 2026-10-18 · 19ff5e0
`lingering` → `leaving` (250 ms, `prefers-reduced-motion` sofort), Cursor springt weiter, Tests/Selbsttest anpassen.
**Fertig wenn:** `just check` grün, `p0sel` grün, installiert. Commit `feat(ui): completed tasks leave open views`.
> Notiz: `leaving` (Map ID → Timer) ersetzt `lingering`; Timer entfernt nur, was dann noch erledigt ist. Abhaken im
> Detail-Panel läuft nicht über die Liste – die Zeile verschwindet dort sofort mit dem Event (Panel bleibt offen).
> WebView: `p0sel`, `base`, `p707` grün (Abhaken bis Frame 15 ms). Installiert 19ff5e0.

### [x] P1-02 · Sync: definierte Auslöser, Debounce mit Höchstwartezeit — 2026-10-18 · 28cc1ce
Scheduler `debounce_ms` 1 500, neu `max_wait_ms` 5 000 (`burst_since`); IPC-Test: jedes schreibende Command genau ein
`local_change`, lesende keins; ADR-022, `sync-strategy.md`.
**Fertig wenn:** `just check` grün, installiert. Commit `feat(sync): upload at most 5 s after a change`.
> Notiz: Scheduler-Tests für Einzeländerung (1,5 s), Serie im Sekundentakt (Läufe bei 5 s und 10 s), letzte Änderung
> einer Serie gedeckelt, Änderung während eines Laufs (genau ein weiterer), Offline/Backoff unverändert. IPC-Test
> `only_writing_commands_trigger_a_sync` prüft alle 14 schreibenden und 5 lesende Commands. ADR-022. Installiert 28cc1ce.

### [x] P1-03 · Nachweis in der echten App — 2026-10-18 · a8c0f99
Selbsttest `p1sync` (Einzeländerung, Serie), Abnahmeskript Abschnitt 6 (A hakt ab → verschwindet, ohne manuellen Sync
oben; B sieht sie unter „Erledigt"), Messwerte in `architecture.md`.
**Fertig wenn:** Selbsttests + ganzes Abnahmeskript grün. Commit `test(desktop): auto-sync and completed-task acceptance`.
> Notiz: `p1sync`: Einzeländerung 1 536 ms, Serie erster Lauf nach 5 003 ms (2 Läufe für 8 Änderungen). Abnahmeskript
> alle 6 Abschnitte grün (Abschnitt 6: oben nach 1 520 ms, B sieht „Erledigt"). Abschnitt 3 robuster gemacht: Der
> Neustart-Kill fand auf entlasteter Maschine eine leere Outbox (Start-Sync mit nur einem Segment schon fertig) – jetzt
> 1 s Backend-Verzögerung für den Neustart; nicht durch P1 verursacht (Upload nach Netz/Start war schon sofort).

---

## P2 · Listen & Kanban-Board

**Ziel:** Tasks lassen sich Listen (z. B. „Einkaufen") zuordnen – schon beim Anlegen. Listen kann man in der UI anlegen,
umbenennen, löschen und umsortieren. Zusätzlich gibt es ein **Kanban-Board aller Listen**: Listen als Spalten
nebeneinander, Tasks als Karten darunter; Ziehen einer Karte in eine andere Spalte ordnet die Task **wirklich** der
anderen Liste zu (Core-`move_task`, synchronisiert). Ein Umschalter „Liste | Kanban" in der Kopfzeile neben „Auswählen"
wechselt die Darstellung.

**Ausgangslage (Stand 2026-10-18):** Core und Shell können Listen anlegen/umbenennen/löschen (`create_task_list`,
`rename_task_list`, `delete_task_list` – Löschen schiebt die Tasks in den Eingang) und Tasks verschieben (`move_task`,
`move_tasks`). Die Listen-Reihenfolge (`TaskList::sort_key`) wird schon synchronisiert (`FieldName::SortKey` in
`sync::list_fields`), lässt sich aber nicht ändern (kein Use-Case, keine Domain-Methode). In der UI fehlen: Umbenennen,
Löschen, Umsortieren von Listen; Listenwahl im Quick-Add (legt heute immer in der offenen Liste bzw. im Eingang an);
das Board. Das Detail-Panel kann eine Task per `<select>` in eine andere Liste verschieben.

**Festlegungen (für alle Aufgaben der Phase):**

- **Begriffe:** Im Code `TaskList` (Domain-Sprache, AGENTS.md §5), in der UI „Liste". Nicht „Kategorie"/„Projekt".
- **Eingang** ist eine Liste wie die anderen, aber: nicht umbenennbar, nicht löschbar, steht **immer zuerst** (in der
  Seitenleiste bei den Ansichten, im Board als erste Spalte) und ist nicht verschiebbar. Benutzerlisten folgen in ihrer
  `sort_key`-Reihenfolge.
- **Board = alle Listen.** Der Umschalter erscheint in Eingang, in Listenansichten und in „Alle Tasks"; in Heute, Geplant
  und Erledigt gibt es nur die Listenansicht (Umschalter ausgeblendet). Im Board-Modus zeigt der Hauptbereich immer das
  Board aller Listen: Ist eine Liste (oder der Eingang) in der Seitenleiste gewählt, wird ihre Spalte hervorgehoben und
  ins Bild gescrollt; bei „Alle Tasks" nichts hervorgehoben. Die Route bleibt die gewählte Ansicht (Zurück/Vor
  funktioniert wie bisher).
- **Board-Inhalt:** je Spalte die **offenen** Tasks der Liste in `sort_key`-Reihenfolge (abgehakte verlassen die Spalte
  wie in P1-01). Fälligkeit/Priorität/Notizen-Hinweis als Meta wie in `TaskRow`.
- **Darstellung merken:** `localStorage` `tasks.layout` = `list` | `board` (per Gerät, über `lib/preferences.ts`, Lesen
  und Schreiben in try/catch wie dort). Standard `list`.
- **Drag & Drop** wie bestehend mit Pointer-Events (kein HTML5-DnD – WebKitGTK-Eigenheiten, siehe `TaskList.startDrag`),
  mit Einfügelinie und Auto-Scroll (horizontal am Board-Rand, vertikal in der Spalte). Immer auch per Tastatur
  (ui-guidelines §5).
- **Ein Core-Aufruf je Aktion:** Karte in andere Spalte = **ein** `move_task(id, { listId, after, before })`;
  Umsortieren in der Spalte ebenso; Spalte/Liste umsortieren = **ein** `move_task_list`. Optimistisch in der UI, bei
  Fehler zurücksetzen (Spalte neu laden) + Fehler-Toast. Verschieben in eine **andere** Liste bekommt einen Toast mit
  Rückgängig („„Milch" nach „Einkaufen" verschoben"), Umsortieren innerhalb einer Liste nicht (wie bisher).
- **Kein O(n) pro Taste/Frame**, Spalten > 200 Karten virtualisiert (feste Kartenhöhe, `VirtualList`), Änderungs-Events
  zentral verteilt (ein `onChange`-Abo für das ganze Board, nicht eines je Spalte).
- **Mehrfachauswahl (P0) nur in der Listenansicht.** Im Board ist „Auswählen" deaktiviert (Tooltip „Nur in der
  Listenansicht"); ein aktiver Auswahlmodus endet beim Umschalten.
- **Detail-Panel** öffnet sich auch aus dem Board (Klick/`Enter` auf eine Karte) – dieselbe `openId`-Logik wie in der
  Liste.
- **Barrierefreiheit:** Board als `role="grid"`-freie Struktur aus Spalten mit `role="list"`/`aria-label="<Listenname>"`
  und Karten `role="listitem"`, fokussierbar (Roving-Tabindex wie die Listenzeilen); Live-Region meldet Verschiebungen
  („„Milch" nach „Einkaufen" verschoben").

### [x] P2-01 · Core: Listen umsortieren — 2026-10-18 · ec72fa9
**Lies vorher:** `crates/todo-core/src/domain/task_list.rs`, `app/rename_task_list.rs`, `app/move_task.rs`
(`insertion_bounds`, FIX-02-Sonderfall), `sync-strategy.md` (Ops), ADR-011, ADR-020.
**Schritte:**
1. Domain: `TaskList::move_to(sort_key, clock)` (setzt `sort_key`, `updated_at` nur bei Änderung – Muster wie
   `Task::move_to`). Inbox: `CoreError::Validation` beim Versuch, sie zu verschieben (Methode `can_move()` analog zu
   `can_delete()`).
2. Use-Case `app/move_task_list.rs`: `MoveTaskList { id, after: Option<TaskListId>, before: Option<TaskListId> }` –
   Semantik der Nachbarn wie `MoveTaskInput` (nur `after`, nur `before`, beide, keins = ans Ende), Nachbarn dürfen nicht
   die Inbox sein, die Liste nicht ihr eigener Nachbar. Reihenfolge der übrigen Benutzerlisten aus
   `repo.list_task_lists()` (wenige Einträge, O(n) ok). Gleiche Nachbar-Keys (paralleles Verschieben auf zwei Geräten):
   wie FIX-02 per `spread_equal_keys`-Muster neu verteilen (dafür die Hilfsfunktion aus `move_task.rs` generisch über
   `(id, sort_key)` machen oder eine Listen-Variante schreiben). **Ein** Commit.
3. `CoreApi::move_task_list(id: String, target: MoveListTargetDto { after?, before? }) -> TaskListDto`; DTO mit ts-rs
   wie `MoveTargetDto`.
4. Prüfen, dass `list_task_lists` nach `sort_key` (dann `id`) sortiert und die Inbox unabhängig davon zuerst liefert
   (ggf. Sortierung im Use-Case/Repository anpassen; Contract-Test `lists_sorted_by_sort_key` erweitern).
5. Tests: Unit (Anfang/Mitte/Ende, unbekannte Liste `NotFound`, Inbox als Liste oder Nachbar `Validation`, gleiche
   Keys, nur `sort_key`-Op im Outbox), Mehrgeräte-Test in `crates/todo-sync/tests/scenarios.rs`: A und B sortieren
   parallel verschiedene Listen um → gleiche Reihenfolge auf beiden.
6. ADR-023 „Listen-Reihenfolge: Inbox fest zuerst, Benutzerlisten per Fractional Index".
**Fertig wenn:** `just check` grün, ADR-023, `interfaces.md` §7 ergänzt. Commit `feat(core): reorder task lists`.
> Notiz: Inbox-zuerst nicht im Repository-Contract, sondern in `CoreApi::list_task_lists` (stabile Sortierung) – das
> Repository bleibt bei `sort_key`, `id`. Gleichstand-Sonderfall direkt im Use-Case per `SortKey::n_between` statt
> `spread_equal_keys` generisch zu machen (Listen sind wenige, kein `TaskSummary`). Keine Format-Änderung.

### [x] P2-02 · Shell & `CoreClient`: `move_task_list` — 2026-10-18 · 5bd247a
**Lies vorher:** P0-02 (Muster), `apps/desktop/src-tauri/src/commands.rs` (Auslöser-Liste, ADR-022),
`tests/commands.rs` (`only_writing_commands_trigger_a_sync`).
**Schritte:**
1. Tauri-Command `move_task_list` (dünn, `async`, ein `state.sync.local_change()`), in `build.rs`, `lib.rs`
   (`generate_handler!`) und `capabilities/default.json` eintragen; in die Auslöser-Liste im Moduldoc von
   `commands.rs` aufnehmen.
2. `CoreClient.moveTaskList(id, target)`; Tauri- und Fake-Implementierung (Fake: gleiche Semantik inkl. Inbox-Regel,
   ein `onChange` mit `lists: [id]`); `just types`.
3. Tests: IPC-Test (Erfolg, Inbox → `validation`) und der Auslöser-Test um `move_task_list` (+1) erweitern; Vitest
   für den Fake.
**Fertig wenn:** `just check` grün. Commit `feat(desktop): move_task_list command`.

### [x] P2-03 · UI: Listen verwalten (umbenennen, löschen, umsortieren) — 2026-10-18 · 3fd0c3f
**Lies vorher:** `features/sidebar/Sidebar.svelte`, `SidebarItem.svelte`, `NewListInput.svelte`,
`lib/lists.svelte.ts`, `components/Menu.svelte`, `components/Dialog.svelte`, ui-guidelines §4/§5.
**Schritte:**
1. **Kontextmenü** auf einem Listeneintrag der Seitenleiste (Rechtsklick und Menütaste/`Umschalt+F10`): „Umbenennen",
   „Nach oben", „Nach unten", „Löschen…". Zusätzlich in der Kopfzeile einer Listenansicht ein Button „Listenaktionen"
   (`Ellipsis`-Icon) mit demselben Menü. Für den Eingang gibt es kein Menü.
2. **Umbenennen** inline in der Seitenleiste (Eingabe wie `NewListInput`, `Enter` speichert, `Esc` bricht ab, leerer
   Name → Fehlermeldung wie beim Anlegen) → `renameTaskList`. Titel in Kopfzeile/Board folgen über `ListsState`.
3. **Löschen** mit Bestätigungs-`Dialog` (destruktiv, ui-guidelines §4): „Liste „Einkaufen" löschen? Ihre n offenen und
   m erledigten Tasks wandern in den Eingang." (Zahlen per `listTasks` der Liste, offene/erledigte getrennt, nur bis 999
   zählen) → `deleteTaskList`. Ist die gelöschte Liste die aktuelle Ansicht → Eingang (bestehender Fallback in
   `App.svelte`). Kein Rückgängig (Core legt die Tasks in den Eingang, die Liste ist per Tombstone weg) – im Dialog
   erwähnen.
4. **Umsortieren** in der Seitenleiste: Ziehen eines Listeneintrags (Griff wie bei Task-Zeilen, Einfügelinie,
   Pointer-Events) und `Alt+↑/↓` auf einem fokussierten Listeneintrag → `moveTaskList`; optimistisch in `ListsState`,
   bei Fehler `reloadLists()`. Eingang nicht verschiebbar.
5. `ListsState` um `rename(id, name)`, `remove(id)`, `move(id, target)` erweitern (optimistisch + Abgleich über
   `onChange`), eingeklappte Seitenleiste: Menü per Rechtsklick auf das Monogramm.
6. Vitest: Umbenennen (Erfolg, leer, `Esc`), Löschen (Dialog, Abbrechen, Bestätigen → Tasks im Eingang, Ansicht wechselt),
   Umsortieren per Tastatur und Ziehen (Reihenfolge im Fake), Eingang ohne Menü.
**Fertig wenn:** `just check` grün; Selbsttest-Schritte in P2-09 vorbereitet. Commit `feat(ui): manage task lists`.
> Notiz: Menü, Dialoge und Umbenennen-Zustand liegen in `App.svelte` (Seitenleiste und Kopfzeile teilen sie). Leerer Name
> beim Inline-Umbenennen = Abbrechen (wie beim Anlegen); im Dialog Fehlermeldung. Fehlgeschlagenes Umbenennen schließt
> das Feld (alter Name, Toast). Kein Rückgängig fürs Löschen (im Dialog genannt). Dabei behoben: `Esc` beim Benennen einer
> neuen Liste legte sie trotzdem an (`blur` nach Abbruch speicherte). Kürzel-Gruppe „Listen" (`Umschalt+F10`,
> `Alt+↑/↓` auf fokussierter Liste – vor den Task-Kürzeln registriert).

### [x] P2-04 · UI: Liste beim Anlegen wählen — 2026-10-18 · 0df92ac
**Lies vorher:** `features/quick-add/QuickAdd.svelte`, `features/task-detail/TaskDetail.svelte` (`moveTo`),
`components/Menu.svelte`.
**Schritte:**
1. Neue Komponente `components/ListPicker.svelte` (oder `features/lists/ListPicker.svelte`): Button mit Listen-Icon
   und Namen („Eingang ▾"), öffnet ein `Menu` mit allen Listen (Eingang zuerst) und am Ende „Neue Liste…" (legt per
   kleinem Eingabefeld an und wählt sie sofort). Tastatur: `Enter`/`Leertaste` öffnet, Pfeile wählen, `Esc` schließt.
2. **Quick-Add** zeigt den Picker rechts in der Eingabezeile. Vorauswahl: die Liste der aktuellen Ansicht, im Eingang
   und in Sammelansichten (Heute/Geplant/Alle) der Eingang, im Board die hervorgehobene Spalte (sonst Eingang). Die Wahl
   gilt für die nächsten Tasks, bis man die Ansicht wechselt. `createTask({ title, listId })`.
   Ist die gewählte Liste nicht die angezeigte, kommt nach dem Anlegen ein kurzer Toast „„Milch" in „Einkaufen"
   angelegt" mit Aktion „Öffnen" (springt zur Liste).
3. **Detail-Panel:** das `<select>` „Liste" durch denselben `ListPicker` ersetzen (gleiches Verhalten, `moveTask` mit
   `listId`), damit die Bedienung überall gleich ist.
4. Kürzel: im Quick-Add-Feld `Alt+L` öffnet den Picker (in `lib/shortcuts.ts` mit `inInputs: true` registrieren,
   Kürzelübersicht Gruppe „Allgemein").
5. Vitest: Vorauswahl je Ansicht, Wahl + Anlegen landet in der Liste (Fake), Toast bei fremder Liste, „Neue Liste…" aus
   dem Picker, Detail-Panel verschiebt über den Picker.
**Fertig wenn:** `just check` grün. Commit `feat(ui): choose the list when adding a task`.
> Notiz: `features/lists/ListPicker.svelte`; „Neue Liste…" fragt per `ListNameDialog` (aus dem Umbenennen-Dialog
> verallgemeinert). Vorauswahl im Board folgt mit P2-06 (hervorgehobene Spalte). Selbsttest `p708a` auf den Picker
> umgestellt. Installation mit P2-05.

### [x] P2-05 · UI: Umschalter Liste | Kanban — 2026-10-18 · 90e6638
**Lies vorher:** `App.svelte` (Kopfzeile, `SelectionBar`, Kürzel), `lib/preferences.ts`, `lib/router.svelte.ts`.
**Schritte:**
1. Segmentierter Umschalter (zwei `IconButton`s mit `pressed`, Icons `List` und `Columns3`/`SquareKanban` aus Lucide –
   einzeln importieren) in der Kopfzeile **links neben „Auswählen"**, `role="group"`, `aria-label="Darstellung"`,
   Tooltips „Liste" / „Kanban" mit Kürzel. Nur in Eingang, Listen und „Alle Tasks" sichtbar.
2. Zustand `layout: "list" | "board"` in `App.svelte`, gespeichert unter `tasks.layout` (`preferences.ts`); Kürzel
   `V` (Gruppe „Ansichten", „Darstellung Liste/Kanban umschalten"), nicht in Textfeldern.
3. Im Board-Modus: Hauptbereich rendert statt `TaskList` das Board (Platzhalter-Komponente bis P2-06), „Auswählen"
   deaktiviert mit Tooltip, laufender Auswahlmodus endet beim Umschalten; Quick-Add bleibt oberhalb.
4. Vitest: Umschalten per Button und `V`, Merken über Neuladen (localStorage), Umschalter fehlt in Heute/Geplant/
   Erledigt, „Auswählen" im Board deaktiviert.
**Fertig wenn:** `just check` grün, keine Änderung an der Listenansicht. Commit `feat(ui): list/board layout switch`.
> Notiz: Icons `List`/`SquareKanban`. Das Board meldet `onready` wie die Liste (sonst fehlten Startmessung und
> Start-Sync, wenn die App im Board-Modus startet). Im Auswahlmodus ist die Kopfzeile die Aktionsleiste – umschalten dort
> per `V`. Nicht installiert (Board noch Platzhalter) – Installation mit P2-06.

### [x] P2-06 · UI: Kanban-Board (Anzeige, Tastatur, Detail) — 2026-10-18 · 70afa3e
**Lies vorher:** `features/task-list/TaskList.svelte`, `TaskRow.svelte`, `task-list-state.svelte.ts` (inkl. `leaving`
aus P1-01, `reload`), `components/VirtualList.svelte`, `lib/lists.svelte.ts`, ui-guidelines §2/§3/§7.
**Schritte:**
1. Neues Feature `features/board/`: `Board.svelte` (horizontal scrollende Spalten, eigene `role`-Struktur),
   `BoardColumn.svelte` (Kopf mit Name, Anzahl offener Tasks, Listenaktionen-Menü aus P2-03; darunter `VirtualList`
   der Karten), `BoardCard.svelte` (runder Erledigt-Haken, Titel einzeilig mit Ellipse, Meta-Zeile Fälligkeit/
   Priorität/Notizen; **feste Höhe** z. B. 64 px für die Virtualisierung), `board-state.svelte.ts`.
2. `BoardState`: je Spalte ein `TaskListState` mit Kontext `{ view: "list", listId }` (Eingang: `inbox`), erste Seite
   100 Karten, weitere beim Scrollen (`loadMore`). **Ein** `onChange`-Abo fürs Board: ≤ 20 IDs → je ID einmal
   `getTask` und das Ergebnis an **alle** Spalten-`apply()` verteilen (jede entscheidet über `inView`), > 20 → alle
   Spalten `reload()`; neue/gelöschte/umbenannte Listen aus `ListsState` → Spalten hinzufügen/entfernen/umbenennen.
   Dafür `TaskListState.attach()` im Board nicht verwenden.
3. Spaltenbreite 280 px, Abstand `--space-3`, Board scrollt horizontal (Mausrad mit `Umschalt`, Touchpad nativ);
   hervorgehobene Spalte (aktuelle Liste) mit Akzent-Rahmen oben und beim Öffnen per `scrollIntoView({ inline:
   "nearest" })` sichtbar. Leere Spalte: dezenter Hinweis „Keine offenen Tasks". Tokens/Design wie Listenzeilen (keine
   Schatten-/Blur-Effekte, §7).
4. Interaktion: Klick auf Karte = Cursor + Detail-Panel (`openId`), Haken = abhaken (verlässt die Spalte, Toast mit
   Rückgängig wie P1-01). Tastatur: `↑/↓`/`J/K` innerhalb der Spalte, `←/→`/`H/L` zur Nachbarspalte (gleiche Höhe,
   begrenzt), `Enter` Detail, `Leertaste`/`E` abhaken, `Entf` löschen mit Rückgängig, `Esc` schließt Detail. Roving-
   Tabindex: genau eine Karte fokussierbar.
5. Kürzel in `App.svelte` mit `when`, die zwischen Liste und Board unterscheiden (keine doppelten Treffer);
   Kürzelübersicht Gruppe „Board".
6. Vitest: Spalten in Listen-Reihenfolge (Eingang zuerst), Karten in `sort_key`-Reihenfolge, nur offene, Abhaken
   verlässt die Spalte, Änderung von anderswo (anderes Gerät/Detail) wandert in die richtige Spalte, neue Liste erscheint
   als Spalte, Tastatur-Navigation zwischen Spalten, Virtualisierung (500 Karten → < 100 DOM-Karten).
**Fertig wenn:** `just check` grün, Board zeigt die Daten korrekt und ist per Tastatur bedienbar (noch ohne Ziehen).
Commit `feat(ui): kanban board`.
> Notiz: Spalten als `listbox` mit Karten `option` (`aria-selected` = Cursor) statt `list`/`listitem` – fokussierbare,
> klickbare Karten brauchen eine interaktive Rolle (wie die Listenzeilen). Erste Seite je Spalte 200 (wie die Liste)
> statt 100. Kartenplatz 64 px (Karte 56 px + Abstand). Quick-Add im Board: Vorauswahl = hervorgehobene Spalte (Route),
> kein „angelegt in"-Toast (alle Listen sichtbar). WebView: `p2board` grün.

### [x] P2-07 · UI: Karten per Drag & Drop zwischen Listen verschieben — 2026-10-18 · 591e725
**Lies vorher:** `TaskList.svelte` (`startDrag`, `moveTo`, Einfügelinie, Auto-Scroll), `TaskListState.reorder`,
`lib/task-actions.ts` (`moveManyWithUndo`), Festlegungen oben.
**Schritte:**
1. Ziehen einer Karte (ganze Karte nach 4 px Bewegung bzw. Griff – Klick bleibt Klick) mit Pointer-Events:
   halbtransparente Kopie am Zeiger, Einfügelinie in der Zielspalte (Index aus Zeiger-Y und Kartenhöhe, inkl.
   Scrollposition), Auto-Scroll horizontal am Board-Rand und vertikal am Spaltenrand (wie `startDrag`), `Esc` bricht
   ab.
2. Ablegen: optimistisch aus der Quellspalte entfernen und in die Zielspalte an die Einfügeposition setzen (neue
   Methode `TaskListState.insertAt(task, index)` bzw. bestehendes `reorder` für dieselbe Spalte), Nachbarn bestimmen →
   **ein** `client.moveTask(id, { listId, after, before })`. Andere Liste → Toast „„X" nach „Y" verschoben ·
   Rückgängig" (über `moveManyWithUndo` mit der Karte als einziger Task – stellt Liste **und** Position wieder her);
   Live-Region meldet die Verschiebung. Fehler → beide Spalten `reload()` + Fehler-Toast. Ablegen auf dem
   Ausgangsplatz = nichts.
3. Tastatur: `Alt+←/→` verschiebt die fokussierte Karte in die Nachbarspalte (an die gleiche Höhe, begrenzt aufs
   Ende), `Alt+↑/↓` innerhalb der Spalte; Fokus bleibt auf der Karte.
4. Zuordnung wirklich geändert: Nach dem Ablegen zeigt die Listenansicht der Zielliste die Task, das Detail-Panel die
   neue Liste; Sync meldet die Änderung (Auslöser `move_task`, ADR-022).
5. Vitest (Fake + jsdom-Maße wie im bestehenden Drag-Test): Ziehen in andere Spalte (Position oben/Mitte/Ende),
   innerhalb der Spalte, Abbrechen mit `Esc`, Fehler setzt zurück, Rückgängig stellt Liste und Position her,
   `Alt+←/→`, gespeicherte Liste im Fake geändert.
**Fertig wenn:** `just check` grün; im Selbsttest (P2-09) verschiebt Ziehen die Task nachweislich (Core-`listId`).
Commit `feat(ui): drag cards between lists`.
> Notiz: Wechsel in eine andere Spalte läuft über `moveManyWithUndo` (ein `moveTasks`-Aufruf mit der einen Karte) –
> damit stellt Rückgängig Liste **und** Platz wieder her; innerhalb der Spalte ein `moveTask` ohne Toast. Ziehen startet
> an der ganzen Karte nach 4 px (der runde Haken startet kein Ziehen). WebView: `p2board` zieht „Käse" nach „Arbeit" und
> prüft die Liste im Core.

### [x] P2-08 · UI: Spalten im Board umsortieren — 2026-10-18 · 5d7b112
**Lies vorher:** P2-03 (Seitenleiste umsortieren), P2-07 (Ziehen), `ListsState`.
**Schritte:**
1. Spaltenkopf ist Griff: Ziehen verschiebt die ganze Spalte (Einfügelinie zwischen Spalten, Auto-Scroll), Eingang
   nicht verschiebbar und immer erste Spalte; Ablegen → **ein** `moveTaskList` (optimistisch in `ListsState`, Board und
   Seitenleiste folgen derselben Reihenfolge).
2. Tastatur: `Alt+←/→` auf dem fokussierten Spaltenkopf.
3. Vitest: Ziehen/Tastatur ändern die Reihenfolge in Board **und** Seitenleiste, Eingang bleibt vorn, Fehler setzt
   zurück.
**Fertig wenn:** `just check` grün. Commit `feat(ui): reorder board columns`.

### [x] P2-09 · Selbsttest, Messung, Doku & Abnahme — 2026-10-18 · 87f6a5e
**Lies vorher:** `apps/desktop/src/selftest.ts` (Suiten `p0sel`, `p1sync`, Helfer `clickWith`, `countCalls`,
`untilAsync`), `tools/acceptance-two-devices.sh`, `tools/install-desktop.sh`.
**Schritte:**
1. Selbsttest-Suite `p2board` (echte WebView, synthetische Ereignisse, keine globalen Eingaben): Listen anlegen,
   umbenennen, umsortieren (Seitenleiste), löschen (Dialog bestätigen → Tasks im Eingang); Quick-Add mit Listenwahl;
   Umschalter auf Kanban (+ Neustart: Darstellung bleibt); Karte per Ziehen in andere Spalte → `getTask(id).listId`
   ist die Zielliste (1 `moveTask`-Aufruf, gezählt mit `countCalls`), Rückgängig; `Alt+→`; Spalte umsortieren.
2. Messung (Debug-Build, wie P0-07): Board mit 10 Listen × 200 Tasks – Zeit bis erste Darstellung (Ziel < 400 ms im
   Release, notieren), Repaint nach Ablegen einer Karte (< 16 / < 50 ms), Scrollen einer Spalte mit 1 000 Karten
   (p95), Leerlauf-CPU im Board-Modus ~0 % gegenüber Listenansicht.
3. Abnahmeskript Abschnitt 7: A legt Liste „Einkaufen" an, verschiebt per Board-Ziehen eine Task hinein, benennt eine
   Liste um und sortiert Listen um; B sieht dieselbe Zuordnung, Namen und Reihenfolge; Neustart A unverändert.
4. Doku: `ui-guidelines.md` §2 (Board-Layout), §4 (`ListPicker`, Board-Komponenten, Umschalter), §5 (neue Kürzel:
   `V`, `Alt+L`, Board-Navigation, `Alt+←/→`); `interfaces.md` (`move_task_list`, `CoreClient.moveTaskList`);
   `architecture.md` §6 (Messwerte); `prototype-acceptance.md` Teil D (Checkliste für den User); AGENTS.md nur
   Status-Zeile.
5. `just desktop-install`; Abnahme durch den User am Desktop (Checkliste Teil D), Funde als FIX.
**Fertig wenn:** `just check`, Selbsttests und ganzes Abnahmeskript grün, Budgets eingehalten, User-Abnahme
eingetragen. Commit `test(desktop): lists and board acceptance`. **Danach:** weiter mit `roadmap-next.md` N0-01.
> Notiz (87f6a5e): Selbsttests `p2board`, `p2layout`, `p2perf` grün; Abnahmeskript alle 7 Abschnitte grün (Abschnitt 7: „Haushalt
> | Obst · Äpfel in Obst" auf A, B und nach Neustart). Messung: Board mit 12 Spalten in 75 ms, Karte verschieben bis
> Repaint p95 17 ms, 1 000 Karten scrollen p95 17 ms (17 DOM-Karten), Leerlauf Board 1,07 % vs. Liste 0,64–0,86 % (Debug-
> Grundlast). Befund beim Testen: Der `localStorage` der WebView gilt für alle Testprofile (`TODO_DATA_DIR`) – die
> Darstellung „Kanban" eines Laufs schlug in den nächsten durch; der Selbsttest stellt jetzt vor jeder Suite die
> Listenansicht her. Doku: `ui-guidelines` §2/§4/§5, `architecture` §6, `prototype-acceptance` A7 + Checkliste D.
> **User-Abnahme am Desktop: bestanden (2026-10-18)**, Checkliste D, keine Funde. Phase P2 abgeschlossen → weiter mit
> `roadmap-next.md` N0-01.

---

## P3 · Kanban-Scrollen & Fenster „Neue Task"

**Ziel:** (1) Im Kanban-Board scrollt `Umschalt` + Mausrad waagerecht. (2) Die Eingabezeile über der Liste entfällt
(mehr Platz); „+ Neue Task" (Seitenleiste, leere Ansicht), `N` und `Strg+N` öffnen ein Fenster in der App („Neue Task",
modaler Dialog, kein OS-Fenster), dessen Inhalt später spezifiziert wird.

### [x] P3-01 · Umschalt + Mausrad im Board — 2026-10-18 · 78ccc88
> Notiz: nicht-passiver `wheel`-Listener am Board, rechnet `deltaMode` (Zeilen/Seiten) um; ohne `Umschalt` scrollt die
> Spalte wie gewohnt. Vitest + WebView (`p2perf`: `scrollLeft` 0 → 300).

### [x] P3-02 · Fenster „Neue Task" statt Eingabezeile — 2026-10-18 · 78ccc88
> Notiz: `features/new-task/NewTaskDialog.svelte`; vorläufiger Inhalt = bisheriger `QuickAdd` (damit man weiter anlegen
> kann): `Enter` legt an und schließt, `Esc` schließt, `Alt+L` Listenwahl. Dabei behoben: Tooltips lagen unter modalen
> Dialogen (jetzt `popover`), verschachtelte Dialoge teilten sich die Titel-ID (jetzt `$props.id()`). Selbsttests auf das
> Fenster umgestellt (`openNewTask()`).

### [x] P3-03 · Inhalt des Fensters „Neue Task" — 2026-10-18 · 648e684
Vorgabe (User, 2026-10-18): bestehendes Fenster überarbeiten, kein neues; kein Formular – die Titelzeile bleibt der
schnelle Weg. Kalender- und GitHub-Anbindung ausdrücklich **nicht** jetzt (→ `roadmap-next.md` N3).
- Titelzeile erkennt (ganze Wörter): `heute/morgen/übermorgen`, Wochentag (`Fr`, `Freitag`; nächster, nie heute; „so"
  nur mit Punkt), `24.10.`/`24.10.26`/`24.10.2026` (ohne Jahr nächstes Vorkommen), `14:00`/`14 Uhr` (ohne Tag heute,
  sonst morgen), `!1`–`!3` (hoch/mittel/niedrig), `@liste` (normalisiert, eindeutiger Anfang). Je Art zählt die letzte
  Angabe; sie fällt samt „am/um/bis" davor aus dem Titel. Weicht der Titel ab, zeigt die Hinweiszeile „Titel: …".
- Chips Fällig (klappt Datum + Uhrzeit + Heute/Morgen/In einer Woche/Entfernen auf), Priorität (Menü), Liste
  (`ListPicker`), Notiz (klappt `Textarea` auf). Von Hand gesetzt gilt vor dem Text.
- `Enter` (Titel) bzw. `Strg+Enter` (Notiz) legt an; „Weitere anlegen" (gemerkt in `localStorage`) lässt das Fenster
  offen und setzt alles außer der Listenwahl zurück. Kürzel `Alt+D/P/L/N`.
- Core: `CreateTaskDto` nimmt optional `notes`, `priority`, `due` → eine Task = ein Commit (erledigt damit den
  Punkt `CreateTaskDto.due` aus N0-03).
> Notiz: `QuickAdd.svelte` entfernt, Parser nach `features/new-task/parse.ts` (`parseNewTask`). Tags fehlen noch, weil
> die Core-API keine Tags kennt (N2). Tests: Parser (10), Dialog (`new-task.test.ts`, 4), Fake-Client; WebView-Selbsttest
> `p3new` (Uhrzeit + Priorität + Notiz über die echte IPC) sowie `p708a`, `p2board`, `p802create` grün. Befund: Der
> Tooltip-Schritt in `p2board` suchte den Listen-Button über `input.parentElement` – jetzt über `aria-label^="Liste: "`.
