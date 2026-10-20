# Notes: Notizen als zweiter Modus der App

Stand: 2026-10-20 · Status: **Entwurf, Fragen mit dem User geklärt** (Abschnitt 0). Die Umsetzung steht in
`roadmap-next.md` unter N5 und wartet dort auf die Freigabe dieses Dokuments. Die Kernentscheidung kommt als ADR-029 in
`tech-decisions.md` (Status „vorgeschlagen").

Dieses Dokument ist die Quelle für alles, was nur Notizen betrifft. Die bestehenden Dokumente (`architecture.md`,
`sync-strategy.md`, `interfaces.md`, `ui-guidelines.md`, `module-structure.md`) verweisen hierher und bekommen die
Details erst mit der Umsetzung der jeweiligen N5-Aufgabe.

## 0. Entscheidungen des Users (Fragerunde 2026-10-20)

| Thema | Entscheidung |
|---|---|
| Ablage in Proton Drive | **Im bestehenden Op-Log** wie Tasks (Segmente, App-Schlüssel, Merge im Core). Keine lesbaren `.md`/`.txt`-Dateien in Drive. `.txt`/`.md` ist der Typ des Inhalts. |
| Konflikt bei Offline-Bearbeitung auf zwei Geräten | **Zeilenbasierter 3-Wege-Merge** gegen die gemeinsame Basis. Nur wenn sich Änderungen überschneiden, entsteht eine **Konfliktkopie**. Kein Text-CRDT. |
| Einstellung `.txt`/`.md` | **Standardtyp für neue Notizen.** Beide Typen liegen gemischt in denselben Ordnern, der Typ einer Notiz ist umstellbar. |
| Organisation | **Verschachtelte Ordner**, beliebig tief. Keine Tags in v1. |
| Reihenfolge im Baum | **Manuell** per Drag & Drop bzw. `Alt+↑/↓` (Fractional Index wie bei Tasks). |
| Name einer Notiz | **Eigenes Feld** wie ein Dateiname. Bei einer neuen Notiz wird er aus der ersten Zeile vorgeschlagen, bis man ihn selbst ändert. |
| Löschen | **Papierkorb** mit Wiederherstellen. Endgültig gelöscht wird manuell oder nach 30 Tagen. |
| Papierkorb geleert, anderes Gerät hat offline eine Notiz in den Ordner gelegt | **Notiz retten:** Sie landet in der Wurzel. |
| Papierkorb geleert, anderes Gerät hat dieselbe Notiz offline bearbeitet | **Delete gewinnt** (wie bei Tasks). |
| Markdown-Darstellung | **Textarea + Vorschau-Umschalter** (`Strg+E`), eigener kleiner Renderer. Kein CodeMirror. |
| Markdown-Umfang der Vorschau | CommonMark-Basis + **GFM-Checklisten (anklickbar), Tabellen**, Durchstreichen, Auto-Links. Kein Mathe, keine Bilder. |
| Editor-Hilfen | **Listen fortsetzen** (+ Tab/Umschalt+Tab) und **Format-Kurzbefehle**. Keine Toolbar, kein Code-Highlighting. |
| Link-Kürzel | `Strg+K` bleibt die Befehlspalette, **Link = `Strg+L`**. |
| Speichern | **Autosave lokal**, Upload gebündelt im Hintergrund. Kein Speichern-Button. |
| Mengen | **Klein, viele:** meist < 20 KB, einige hundert bis ~2 000 Notizen. |
| Layout im Notes-Modus | **2 Spalten:** Seitenleiste = Ordnerbaum mit Notizen, Hauptbereich = Editor. |
| Zustand beim Wechsel | **Alles merken** je Modus, auch über Neustart (pro Gerät). Die App startet im zuletzt benutzten Modus. |
| Umschalten | **Klick auf „Tasks"/„Notes"** neben dem Logo, außerdem **`Strg+1` / `Strg+2`**. |
| Fokus/Status | **Zen-Modus**, **begrenzte Zeilenbreite**, **Statuszeile**. Keine Breadcrumb-Kopfzeile. |
| Schrift | `.txt` in **Monospace**, `.md` **proportional** (UI-Schrift). |
| Verbindung zu Tasks in v1 | **Gemeinsame Suche** (Befehlspalette findet Tasks und Notizen). `[[Verknüpfungen]]` erst mit N3. |
| Dateien | **Ordner importieren** (einmalige Kopie, nur Desktop). Kein Export in v1. |
| Anhänge | Nicht in v1. |
| Einstellungen | **Pro Gerät** (`localStorage`), nicht synchronisiert. |
| Reihenfolge in der Roadmap | **N0 → Notes (N5) → N4 → Web (N1)**. Web bringt Notes direkt mit. |

Nachgefragt und bestätigt (zweite Runde, 2026-10-20):
- Überschriften-Kürzel `Strg+Alt+1…3` (`Strg+1/2` wechselt den Modus).
- Upload von Notiz-Inhalten: Debounce 3 s, höchstens 30 s, sofort bei Notiz-/Moduswechsel, Fokusverlust, Schließen (§4.6).
- Eine neue Notiz, die leer verlassen wird, wird verworfen.
- Rechtschreibprüfung in v1 aus, später als Einstellung.

Von mir festgelegt (Einspruch jederzeit möglich): Größenlimit 1 MiB pro Notiz (§3.4).

## 1. Ziel und Nicht-Ziele

**Ziel:** Ein zweiter Arbeitsbereich „Notes" in derselben App mit derselben Seitenleiste. Notizen sind Text- oder
Markdown-Dokumente in Ordnern, offline-first, Ende-zu-Ende-verschlüsselt über Proton Drive synchronisiert, mit einem
großen, ruhigen Editor. Der Wechsel zwischen Tasks und Notes ist ein Klick und verliert keinen Zustand.

**Nicht-Ziele (v1):** Echtzeit-Co-Editing, Text-CRDT, Anhänge/Bilder, Mathe, Code-Highlighting, Export,
`[[Verknüpfungen]]`, Tags, Versionsverlauf als Ansicht, lesbare Dateien in Drive, Zwei-Wege-Abgleich mit einem
lokalen Ordner.

## 2. Architektur-Erweiterung

Notes ist **kein** eigenes Subsystem. Es ist ein weiterer Satz Entitäten im selben Core, im selben Op-Log und in
derselben SQLite-Datei. Alle Prinzipien aus `AGENTS.md` §3 gelten unverändert. Insbesondere gibt es keinen
Proton-Code außerhalb des Adapters, und `RemoteStore` bleibt unverändert, weil Notizen nur neue Op-Inhalte sind.

```
packages/ui/features/notes/*  ──►  CoreClient (+ Notes-Methoden)  ──►  IPC | WASM
                                                                          │
todo-core:  domain/note.rs, domain/note_folder.rs, domain/note_tree.rs     ▼
            sync/revision.rs (Revisionen + 3-Wege-Merge, rein, ohne IO)   CoreApi (+ Notes)
            app/*_note*.rs (Use-Cases)                                    │
            ports/note_repository.rs (Queries)                            ▼
todo-sync:  unverändert bis auf Segment-Byte-Limit + Push-Timing für Notizen
todo-storage-sqlite: Migration notes/note_folders/note_revisions/notes_fts
```

| Schicht | Neu |
|---|---|
| Domain | `Note`, `NoteFolder`, `NoteKind { Text, Markdown }`, `NoteId`/`NoteFolderId` (UUIDv7), `NoteName` (validiert), Baum-Regeln (Zyklen, Waisen, wirksamer Papierkorb) als reine Funktionen |
| Sync-Modell | `EntityRef::Note`, `EntityRef::NoteFolder` (angehängt), neue `FieldName`s (angehängt), neue `OpKind::Revise` (angehängt) für Inhalte, `RevisionSet` + `resolve_body` (§4) |
| Use-Cases | Notiz/Ordner anlegen, Inhalt speichern (mit Basis-Revision), umbenennen, Typ ändern, verschieben, in den Papierkorb, wiederherstellen, endgültig löschen, Papierkorb leeren, Auto-Leeren nach 30 Tagen, Konflikt-Folge-Commit, Import |
| Ports | `ChangeSet` bekommt Notiz-/Ordner-Änderungen (eine Transaktion mit Tasks). Neuer Lese-Port `NoteRepository` (Baum seitenweise, Notiz laden, Papierkorb, `search_notes`, Revisionen) mit Standard-Implementierungen wie bei `search_tasks` (ADR-027). Die SQLite-Implementierung implementiert beide Traits. |
| Shell (Desktop) | Commands 1:1 zu den Use-Cases, Auslöser-Liste für den Scheduler (ADR-022) ergänzt, Import-Command mit Ordnerauswahl |
| UI | `features/notes/` (Baum, Editor, Vorschau, Papierkorb), App-Modus in `lib/app-mode.svelte.ts`, Umschalter in `Sidebar.svelte`. Lazy geladener Chunk. |

Warum ein eigener Lese-Port statt `TaskRepository` zu erweitern: `TaskRepository` ist schon groß, und die
Notiz-Queries haben andere Formen (Baum statt Seiten mit Smart Views). **Schreiben** bleibt bei
`TaskRepository::commit`, weil Zustand und Outbox-Ops in **einer** Transaktion entstehen müssen (ADR-015). Ob der Trait
dafür in `Repository` umbenannt wird, entscheidet N5-04 (eigener Commit, rein mechanisch).

## 3. Datenmodell

### 3.1 Entitäten

```rust
pub struct NoteFolder {
    pub id: NoteFolderId,
    pub parent: Option<NoteFolderId>,   // None = Wurzel
    pub name: NoteName,
    pub sort_key: SortKey,              // Reihenfolge unter Geschwistern (Ordner und Notizen gemeinsam)
    pub created_at: Timestamp,
    pub trashed_at: Option<Timestamp>,  // Some = im Papierkorb
}

pub struct Note {
    pub id: NoteId,
    pub folder: Option<NoteFolderId>,   // None = Wurzel
    pub name: NoteName,                 // ohne Endung; Endung folgt aus `kind`
    pub auto_name: bool,                // true: Name folgt der ersten Zeile (bis zum ersten Umbenennen)
    pub kind: NoteKind,                 // Text (.txt) | Markdown (.md)
    pub body: String,                   // aufgelöster Inhalt (§4.3), LF-Zeilenenden
    pub body_rev: RevisionId,           // Revision, auf der `body` beruht (für den Editor)
    pub sort_key: SortKey,
    pub created_at: Timestamp,
    pub modified_at: Timestamp,         // Wall-Zeit der neuesten Inhalts- oder Namensänderung (abgeleitet)
    pub trashed_at: Option<Timestamp>,
}
```

- **Ordner und Notizen teilen sich die Reihenfolge** unter einem Elternordner (ein `sort_key`-Raum), damit man
  Notizen frei zwischen Unterordner schieben kann. Die Darstellung zeigt trotzdem keine Gruppierung „Ordner zuerst".
  Neue Elemente kommen ans Ende.
- **`NoteName`:** getrimmt, 1–200 Zeichen, ohne `/`, `\`, Steuerzeichen und ohne führenden Punkt. Namen müssen
  **nicht eindeutig** sein, weil der Sync gleichnamige Anlagen auf zwei Geräten nicht verhindern kann. Die UI warnt
  beim Umbenennen auf einen vorhandenen Namen im selben Ordner, verbietet es aber nicht.
- **Auto-Name:** Solange `auto_name` gilt, setzt der Use-Case „Inhalt speichern" den Namen aus der ersten nicht
  leeren Zeile (ohne führende `#`, `-`, `*`, `>` und Leerzeichen, auf 60 Zeichen gekürzt, leer → „Unbenannt").
  Umbenennen von Hand setzt `auto_name = false`. Weil das im Core passiert, kommen alle Geräte zum selben Ergebnis.
- **Typwechsel** `.txt` ↔ `.md` ändert nur `kind`, nie den Inhalt.

### 3.2 Baum-Regeln (Materialisieren, ohne eigene Ops)

1. **Waisen:** Ist der Elternordner gelöscht (Tombstone) oder (noch) unbekannt, erscheint das Element in der
   **Wurzel**. Das ist die Antwort „Notiz retten". Ein später eintreffendes `Create` des Ordners hängt es wieder ein.
2. **Zyklen** (A verschiebt X nach Y, B offline Y nach X): Beim Materialisieren wird der Zyklus gesucht. Der Ordner im
   Zyklus, dessen `ParentFolder`-Register die **höchste HLC** hat, wird in die Wurzel gehängt. Das ist deterministisch,
   also sehen alle Geräte denselben Baum. Es entsteht keine Op, bis jemand den Ordner bewusst verschiebt.
3. **Wirksamer Papierkorb:** Ein Element ist im Papierkorb, wenn es selbst oder ein Vorfahr `trashed_at` hat. Die Liste
   „Papierkorb" zeigt nur die **obersten** solchen Elemente (der Ordner, nicht jede Notiz darin).

### 3.3 Revisionen (Inhalt)

```rust
pub struct RevisionId(OpId);            // die Op, die die Revision geschrieben hat – global eindeutig
pub struct Revision {
    pub id: RevisionId,
    pub hlc: Hlc,
    pub parents: Vec<RevisionId>,       // leer = erste Revision; ≥ 2 = Merge-Revision
    pub text: String,
}
```

Der Inhalt einer Notiz ist **kein LWW-Feld**, sondern eine wachsende Menge von Revisionen (G-Set, §4).

### 3.4 Grenzen

| Grenze | Wert | Verhalten |
|---|---|---|
| Notiz-Inhalt | 1 MiB UTF-8 | darüber `CoreError::Validation`; die UI warnt ab 256 KB in der Statuszeile |
| Ordner-Tiefe | 32 | `Validation` beim Verschieben/Anlegen |
| Name | 200 Zeichen | siehe `NoteName` |

### 3.5 SQLite (Migration in N5-04)

| Tabelle | Inhalt |
|---|---|
| `note_folders` | `id, parent_id, name, sort_key, created_at, trashed_at` + Index `(parent_id, sort_key)` |
| `notes` | `id, folder_id, name, auto_name, kind, body, body_rev, size, sort_key, created_at, modified_at, trashed_at` + Index `(folder_id, sort_key)`, `(trashed_at)` |
| `note_revisions` | `note_id, rev_device, rev_seq, hlc, parents BLOB (postcard), text` – Primärschlüssel `(note_id, rev_device, rev_seq)`; hält nur Revisionen, die noch als Basis gebraucht werden können (§4.5) |
| `notes_fts` | FTS5 `trigram case_sensitive 0` über `name, body`, externer Inhalt, Trigger nur bei geändertem Namen/Inhalt (wie ADR-027) |

`notes.body` ist der **aufgelöste** Text. Der Baum und das Öffnen einer Notiz lesen nur `notes`, nie die Revisionen.

## 4. Sync-Strategie für Notizen

### 4.1 Was über den Draht geht

- `EntityRef::NoteFolder(id)`, `EntityRef::Note(id)` – **angehängt** an das Enum (Formatregel ADR-016).
- Normale LWW-Felder (`Create`/`Set`) mit neuen, **angehängten** `FieldName`-Codes (nächste freie, heute ab 13):
  `ParentFolder`, `NoteKind`, `AutoName`, `Trashed`. Wiederverwendet werden `Name` (11), `SortKey` (9),
  `CreatedAt` (10), und für Notizen der Ordner-Verweis über `ParentFolder`.
- **Inhalt:** neue Op-Art `OpKind::Revise { parents: Vec<RevisionId>, text: String }` (angehängt). Eine Revision ist
  unveränderlich. Ihre ID ist die `OpId` der Op.
- `Delete` wie bisher (Tombstone, Delete gewinnt), nur für endgültiges Löschen.

**Ältere App-Versionen** können solche Segmente nicht dekodieren (unbekannte Variante) und überspringen sie mit dem
Hinweis „App aktualisieren" (`sync-strategy.md` §5). Der Cursor rückt dort nicht vor, nach dem Update werden die
Segmente nachgelesen. Hinweis in README/Release-Notizen: **alle Geräte gleichzeitig aktualisieren** (wie N4).

### 4.2 Merge der Revisionen (konvergent)

- Die Revisionen einer Notiz bilden einen **DAG** (Eltern-Verweise). Empfangen heißt **Vereinigung**. Die ist
  kommutativ, assoziativ und idempotent, also gilt die Konvergenz des bestehenden Merges weiter.
- **Köpfe** = Revisionen ohne bekannten Nachfolger. Ein Kopf: Das ist der Inhalt. Mehrere Köpfe heißt, es wurde
  nebenläufig bearbeitet.
- Eine lokale Bearbeitung schreibt eine Revision mit `parents` = **die Basis, die der Editor geladen hatte**
  (`body_rev`). Nicht „die aktuellen Köpfe", sonst würde eine Remote-Änderung, die während des Tippens eintraf,
  stillschweigend überschrieben.

### 4.3 Auflösung mehrerer Köpfe (`resolve_body`, reine Funktion im Core)

1. Köpfe nach HLC aufsteigend sortieren: `h1 < h2 < …`.
2. Falten: `acc = h1`; für jeden weiteren Kopf `h`: Basis = **jüngster gemeinsamer Vorfahr** von `acc` und `h`
   (bei mehreren Kandidaten der mit der höchsten HLC), `acc = merge3(basis, acc, h)`.
3. `merge3` ist ein **zeilenbasierter diff3**. Er ist deterministisch und hängt nur von den drei Texten ab, nicht vom
   Gerät. Die Reihenfolge der Operanden ist durch die HLC-Sortierung festgelegt. Damit rechnet jedes Gerät mit
   derselben Revisionsmenge denselben Text aus.
4. **Sauber** (keine überlappenden Hunks): Ergebnis = zusammengeführter Text.
5. **Konflikt** (überlappende Hunks) oder **Basis nicht mehr vorhanden** (weggeräumt, §4.5): Der Text des Kopfes mit
   der **höheren HLC** gewinnt. Der andere wird **Konfliktkopie** (§4.4).

Kandidat für diff3 ist die Crate `diffy` (reines Rust, MIT/Apache, Merge eingebaut). Größe und wasm32-Build prüft
N5-02, die Entscheidung kommt ins ADR. Fällt `diffy` durch, kommt ein eigener Myers-Diff mit diff3 in
`sync/revision.rs` (~400 Zeilen). Ob eine Änderung **direkt neben** einer anderen als Konflikt gilt, legen die Tests
in N5-02 fest. Vorschlag: benachbarte Zeilen sind kein Konflikt, dieselbe Zeile schon.

### 4.4 Folge-Commit nach dem Pull: Köpfe zusammenlegen

Damit die Köpfe nicht dauerhaft mehrere bleiben und jedes Öffnen neu mergt, startet die Sync-Engine nach
`apply_remote` für jede Notiz mit > 1 Kopf den Use-Case `ResolveNoteHeads`. Er schreibt in **einem** Commit:

- eine **Merge-Revision** mit `parents` = alle Köpfe und `text` = Ergebnis von `resolve_body`;
- bei Konflikt zusätzlich eine **Konfliktkopie**: neue Notiz im selben Ordner, direkt hinter dem Original, Name
  „<Name> (Konflikt <Gerätename>, <Datum>)", Inhalt = Text des unterlegenen Kopfes, gleicher Typ. Ihre ID wird
  **deterministisch** aus `(Notiz-ID, Revisions-ID des unterlegenen Kopfes)` abgeleitet (UUIDv5, gleiche Ausnahme wie
  in N4/ADR-028).

Lösen zwei Geräte denselben Konflikt gleichzeitig auf, entstehen zwei Merge-Revisionen mit **gleichem Text** und
dieselbe Kopie (gleiche ID, `Create` mergt feldweise). Die beiden Merge-Revisionen sind wieder zwei Köpfe mit gleichem
Text. Das löst sich trivial auf (diff3 identischer Texte), es entsteht keine weitere Kopie, und die nächste Bearbeitung
legt sie zusammen. Toast: „Konflikt in „Blatt 3" – Kopie angelegt" mit „Anzeigen".

### 4.5 Aufräumen der Revisionen

- Lokal bleiben gespeichert: alle Köpfe und jede Revision, die noch **gemeinsamer Vorfahr** einer künftigen
  nebenläufigen Bearbeitung sein kann.
- Regel: Eine Revision `R` darf weg, sobald es einen Nachfahren `D` gibt und **alle aktiven Geräte** (aus
  `devices/*.json` mit `cursors`, aktiv = `last_seen` < 90 Tage) die Op von `D` gesehen haben. Jede spätere Bearbeitung
  dieser Geräte stammt dann von `D` ab, also liegt jeder künftige gemeinsame Vorfahr bei `D` oder später.
- **Snapshots** enthalten je Notiz die Köpfe und deren gemeinsame Vorfahren. Ältere Revisionen enthalten sie nicht,
  weil die Segmente sie bis zur Kompaktierung noch haben.
- Fehlt trotzdem eine Basis (Gerät > 90 Tage offline), greift §4.3 Punkt 5: Es entsteht eine Konfliktkopie, kein
  Datenverlust.

### 4.6 Outbox und Upload

- **Zusammenfassen in der Outbox:** Autosave schreibt lokal alle ~400 ms. Liegt für dieselbe Notiz schon eine
  **ungepushte** `Revise`-Op in der Outbox, wird sie **ersetzt** (gleiche `seq`, neue HLC, neuer Text, `parents` der
  ersetzten Op). So geht pro Upload höchstens eine Revision je Notiz raus. Das betrifft `record_local` bzw. die
  Outbox-Buchhaltung im Repository (ADR-015) und kommt als Nachtrag ins ADR-029.
- **Timing:** Für Inhalte gilt Debounce 3 s, Höchstwartezeit **30 s** während durchgehenden Tippens (Tasks: 1,5 s /
  5 s, ADR-022). **Sofort** wird gepusht beim Wechsel der Notiz, beim Wechsel des Modus, wenn das Fenster den Fokus
  verliert oder verborgen wird und beim Schließen. Begründung: Eine Stunde Tippen erzeugt sonst ~720 Volltext-Uploads.
- **Segment-Größe:** Die Engine begrenzt Segmente zusätzlich auf **2 MiB Klartext** (bisher nur 500 Ops). Eine
  einzelne Revision ≤ 1 MiB passt immer.
- **Später (nicht v1):** Revisionen als Delta zur ersten Elternrevision übertragen. Das spart Upload, kostet aber
  Komplexität beim Aufräumen.

### 4.7 Papierkorb und endgültiges Löschen

- In den Papierkorb legen heißt `Set Trashed = Some(now)`, ein normales LWW-Feld, also **kein** Tombstone.
  Wiederherstellen heißt `Set Trashed = None`. Bearbeitung und Papierkorb stören sich nicht. B kann eine Notiz
  bearbeiten, die A gerade in den Papierkorb gelegt hat, und die Bearbeitung bleibt erhalten.
- Wiederherstellen eines Elements, dessen Elternordner selbst im Papierkorb liegt oder gelöscht ist, legt es in die
  **Wurzel**.
- **Endgültig löschen** (einzeln, „Papierkorb leeren" oder automatisch) heißt `Delete` für das Element **und alle
  bekannten Nachfahren**, in einem Commit. Nachfahren, die das löschende Gerät nicht kannte, werden Waisen (§3.2,
  „Notiz retten"). Eine Bearbeitung, die das löschende Gerät nicht kannte, geht verloren (Delete gewinnt, User-
  Entscheidung). Das ist selten, weil endgültiges Löschen nur aus dem Papierkorb passiert.
- **Auto-Leeren:** Beim Start und danach höchstens einmal täglich, nie per Timer im Leerlauf, löscht jedes Gerät
  Elemente mit `trashed_at` älter als 30 Tage. Doppelte Deletes von zwei Geräten sind harmlos.

### 4.8 Tests (Pflicht, vgl. `sync-strategy.md` §7)

- Property-Test **Konvergenz** mit Revisionen: zufällige Bearbeitungen auf 2–4 Geräten, zufällige Liefer-Reihenfolge,
  Duplikate, Folge-Commits. Am Ende ist der aufgelöste Text auf allen Geräten gleich, und die Anzahl Konfliktkopien
  ist gleich.
- Property-Test `merge3`: Ändern beide Seiten disjunkte Zeilenbereiche, enthält das Ergebnis beide Änderungen. Ist
  eine Seite gleich der Basis, ist das Ergebnis die andere Seite.
- Szenario-Tests (`todo-sync/tests/scenarios.rs`): A ändert Absatz 1, B Absatz 3 → ein Text mit beiden, keine Kopie;
  A und B ändern dieselbe Zeile → eine Kopie auf beiden Geräten; A leert den Papierkorb, B legt offline eine Notiz in
  den Ordner → Notiz in der Wurzel; Zyklus durch Verschieben → gleicher Baum auf beiden.
- Golden Files: `ops_v1.bin` bleibt lesbar, neues `ops_v1_notes.bin`. Ein Test mit unbekannter Variante zeigt
  „überspringen", keinen Absturz.

## 5. UI-Flow: Tasks ↔ Notes

### 5.1 Umschalter

```
┌─ [◆] Tasks ⇄ ────┐        Klick / Strg+2        ┌─ [◆] Notes ⇄ ────┐
│ [+ Neue Task]     │  ───────────────────────►   │ [+ Neue Notiz]    │
│ ▸ Eingang      12 │                             │ ▾ Uni             │
│ ▸ Heute         3 │  ◄───────────────────────   │   ▾ Analysis      │
│ …                 │        Klick / Strg+1        │     ▤ Blatt 3  ◀  │
```

- Der Name neben dem Logo wird ein **Button** (`<button>` mit sichtbarem Fokus). Er zeigt den aktiven Modus und ein
  kleines `⇄`-Icon, das das Umschalten andeutet. Tooltip: „Zu Notes wechseln (Strg+2)" bzw. „Zu Tasks wechseln
  (Strg+1)". `aria-label` entsprechend. In der eingeklappten Seitenleiste (56 px) wird daraus ein Icon-Button.
- `Strg+1` / `Strg+2` wirken **überall**, auch im Editor und in Textfeldern. In der Befehlspalette gibt es „Zu
  Tasks/Notes wechseln", „Neue Notiz" und „Neuer Ordner".
- **Übergang:** Seitenleisten-Inhalt und Hauptbereich blenden in ≤ 160 ms über (nur `opacity`). Der Kopf der
  Seitenleiste und die Sync-Fußzeile bleiben stehen, damit der Wechsel ruhig wirkt. Bei `prefers-reduced-motion`
  gibt es keinen Übergang.
- Der Primärbutton oben wechselt mit dem Modus: „Neue Task" ↔ „Neue Notiz". `N`/`Strg+N` wirken jeweils im aktiven
  Modus.

### 5.2 Zustand merken

- **Im Speicher:** Der Tasks-Teilbaum bleibt beim Wechsel **gemountet** und wird nur versteckt (`hidden`). Dadurch
  bleiben Ansicht, Auswahl, Detail-Panel und Scrollposition ohne Extra-Code erhalten. Der Notes-Teilbaum wird beim
  ersten Wechsel geladen (eigener Chunk) und bleibt danach ebenfalls gemountet. Kosten (RAM, DOM) misst N5-12. Wird
  das Budget gerissen, wird stattdessen der Zustand serialisiert und neu gemountet.
- **Über Neustart (pro Gerät, `localStorage`, alle Zugriffe in `try/catch`):** `tasks.mode` (`tasks`|`notes`),
  `notes.open` (ID der offenen Notiz), `notes.expanded` (aufgeklappte Ordner), `notes.view` (je Notiz:
  Bearbeiten/Vorschau, Auswahl, Scroll; LRU mit 50 Einträgen), `notes.zen`. Der Tasks-Zustand bleibt wie heute
  (`tasks.layout` usw.).
- **Start im Notes-Modus:** Der Notes-Chunk wird sofort mitgeladen. Für „Kaltstart bis zur ersten interaktiven Notiz"
  gilt dasselbe Budget wie für die Liste (< 400 ms). Existiert die gemerkte Notiz nicht mehr (gelöscht, im
  Papierkorb), zeigt der Editor den leeren Zustand.

### 5.3 Seitenleiste im Notes-Modus

```
┌─ [◆] Notes ⇄ ───────┐
│ [+ Neue Notiz]   ⋯   │  ⋯ = Neuer Ordner, Ordner importieren…
│ ▾ Uni                │
│   ▾ Analysis         │  Einrückung 16 px je Ebene, Zeilen 32 px
│     ▤ Blatt 3     ◀  │  ▤ .txt / ⌗ .md (Icon), offene Notiz = aktive Navigation
│     ⌗ Grenzwerte     │
│   ▸ LinAlg           │
│ ▸ Privat             │
│ ▤ Einkauf            │  Endung nur als Icon, nicht im Text
│ ─────────────────── │
│ 🗑 Papierkorb      3 │
│ ⟳ Synchronisiert     │
└──────────────────────┘
```

- **Virtualisiert** als flache Liste sichtbarer Zeilen (gleiche `VirtualList` wie die Tasks). Der Inhalt eines Ordners
  wird erst beim Aufklappen geladen (seitenweise, §6 Performance) und über `ChangeEvent`s aktuell gehalten.
- **Maus:** Klick auf eine Notiz öffnet sie, Klick auf einen Ordner klappt auf/zu, Doppelklick benennt um.
  Drag & Drop **verschiebt** (in einen Ordner = hinein, zwischen Zeilen = umsortieren, mit Einfügelinie).
  Kontextmenü (Rechtsklick, `Umschalt+F10`, Menütaste): Neue Notiz hier, Neuer Ordner hier, Umbenennen, Typ:
  Markdown/Text, Verschieben nach…, In den Papierkorb.
- **Tastatur:** `↑/↓`, `J/K` bewegen; `→`/`←` klappen auf/zu bzw. springen zum Elternordner; `Enter` öffnet die Notiz
  und fokussiert den Editor; `F2` benennt um; `Entf` legt in den Papierkorb (Toast mit „Rückgängig"); `Alt+↑/↓`
  sortiert um; `M` öffnet „Verschieben nach…" (Ordnerauswahl mit Suche, wie bei Tasks).
- **Neue Notiz** (`N`, `Strg+N`, Button): Sie wird im Ordner der aktuell offenen bzw. fokussierten Notiz angelegt,
  sonst in der Wurzel, am Ende. Typ = Einstellung „Standardtyp". Name „Unbenannt" mit `auto_name`. Der Editor wird
  sofort fokussiert. Bleibt die neue Notiz leer und verlässt man sie, wird sie verworfen (keine leeren Leichen).
- **Papierkorb** (Eintrag unten, Zähler) öffnet im Hauptbereich eine Liste der obersten gelöschten Elemente
  (Name, Ordnerpfad, „gelöscht vor 3 Tagen") mit „Wiederherstellen", „Endgültig löschen" und oben „Papierkorb
  leeren" (Bestätigungsdialog, weil nicht rückgängig zu machen). Notizen im Papierkorb lassen sich nur lesen.

### 5.4 Hauptbereich

- **Keine Notiz offen:** ruhiger leerer Zustand („Keine Notiz geöffnet") mit „Neue Notiz" und Kurzbefehlen.
- **Notiz offen:** Editor (§6) über die volle Breite und Höhe. Kein Detail-Panel.

## 6. Editor-Anforderungen

### 6.1 Aufbau

```
┌────────────────────────────────────────────────────────────────┐
│                                                                │
│        # Blatt 3                                               │  Textspalte max. 72ch,
│                                                                │  zentriert (abschaltbar)
│        - Aufgabe 1: Konvergenz der Reihe …                     │
│        - [ ] Aufgabe 2                                         │
│                                                                │
│────────────────────────────────────────────────────────────────│
│ Uni › Analysis › Blatt 3.md   Gespeichert · 312 Wörter · 1 840 Z.   [Bearbeiten|Vorschau] ⋯ │
└────────────────────────────────────────────────────────────────┘
```

- **Eingabefeld:** ein natives `<textarea>`, das den Hauptbereich füllt und selbst scrollt (keine Höhen-Automatik,
  die bei jedem Tastendruck das Layout neu rechnet). `.md` in der UI-Schrift, `.txt` in `--font-mono`. Zeilenhöhe
  1,6, Schriftgröße über ein Token (`--font-size-editor`, Standard 15 px). Rechtschreibprüfung in v1 aus (`spellcheck="false"`).
- **Statuszeile** (28 px, unten, gedimmte Schrift): Ordnerpfad und Name mit Endung (Klick auf den Namen benennt
  um), Speicher-/Sync-Zustand, Wörter, Zeichen, bei Auswahl „n ausgewählt", Umschalter Bearbeiten/Vorschau (nur
  `.md`) und `⋯`-Menü (Umbenennen, Typ ändern, Verschieben nach…, In den Papierkorb). Die Zähler werden 300 ms nach
  der letzten Eingabe berechnet, nicht bei jedem Tastendruck (§4 in AGENTS: keine O(n)-Arbeit pro Taste).
- **Speicher-Zustand:** „Gespeichert" (lokal committet), „Speichert…" (zwischen Eingabe und Commit), „Nicht
  synchronisiert" (Outbox enthält die Revision, offline), „Synchronisiert". Kein Spinner und keine Animation.
- **Zen-Modus** (`Strg+Umschalt+F`, Palette, `⋯`): Seitenleiste ausblenden, Statuszeile nur bei Mausbewegung oder
  Fokus zeigen. `Esc` beendet ihn (das erste `Esc` im Editor beendet zuerst Zen). Wird pro Gerät gemerkt.
- **Begrenzte Zeilenbreite:** Standard an (72ch), Einstellung „Zeilenbreite begrenzen".

### 6.2 Speichern (Autosave)

1. Eingabe → Puffer in der UI. **400 ms** nach der letzten Eingabe → `updateNoteBody(id, text, baseRev)`.
2. Sofort gespeichert wird (ohne Debounce) beim Wechsel der Notiz, beim Wechsel des Modus, bei `blur`, bei
   `visibilitychange` → verborgen und vor dem Schließen des Fensters. Die Desktop-Shell wartet beim Schließen auf
   diesen letzten Commit (mit Timeout).
3. Der Core antwortet mit `{ rev, body }`. `body` weicht vom gesendeten Text nur ab, wenn inzwischen eine
   Remote-Revision eintraf und gemergt wurde.
4. Während ein Speichern läuft, wird nicht parallel ein zweites gestartet. Weitere Eingaben sammeln sich und gehen
   danach mit dem neuen `baseRev` raus.

### 6.3 Remote-Änderung an der offenen Notiz

- Kommt per `ChangeEvent` eine neue Revision der offenen Notiz:
  - **Puffer unverändert seit dem letzten Speichern:** neuen Text übernehmen.
  - **Puffer hat ungespeicherte Eingaben:** sofort speichern (Basis = alte `baseRev`). Der Core mergt (§4.3) und
    liefert den Ergebnistext.
- Text im Feld ersetzen, ohne den Cursor zu verlieren: gemeinsamen Präfix und Suffix von altem und neuem Text
  bestimmen und Auswahl und Scroll entsprechend verschieben. Die Ersetzung geht über `setRangeText` auf den
  geänderten Bereich, nicht über `value =`, damit nicht das ganze Feld neu gesetzt wird.
- Konfliktkopie entstanden: Toast „Konflikt – Kopie „Blatt 3 (Konflikt Laptop, 20.10.)" angelegt" mit „Anzeigen".

### 6.4 Editor-Hilfen und Kurzbefehle

Alle Text-Einfügungen laufen über `document.execCommand("insertText")` (bzw. `setRangeText` mit anschließendem
`input`-Event, wo das nicht geht), damit **`Strg+Z` / `Strg+Umschalt+Z`** (natives Undo) weiter funktionieren. Prüfen
in N5-08, ob WebKitGTK `insertText` in Textareas unterstützt. Während einer IME-Komposition (`isComposing`) greift
keine Hilfe.

| Aktion | Kürzel | Gilt für |
|---|---|---|
| Liste fortsetzen (`- `, `* `, `1. ` → `2. `, `- [ ] `, `> `) | `Enter` am Ende eines Listenpunkts; leerer Punkt + `Enter` beendet die Liste | .md und .txt |
| Einrücken / Ausrücken (Auswahl zeilenweise) | `Tab` / `Umschalt+Tab` | .md: 2 Leerzeichen; .txt: Tabulator |
| Editor verlassen (statt Tab-Einfügen) | `Esc`, dann `Tab` | beide (Barrierefreiheit) |
| Fett / Kursiv / Durchgestrichen | `Strg+B` / `Strg+I` / `Strg+Umschalt+S` | .md |
| Link | `Strg+L` → `[Auswahl](|)`; URL über markiertem Text einfügen → Link | .md |
| Überschrift 1–3 / Absatz | `Strg+Alt+1…3` / `Strg+Alt+0` | .md |
| Checkbox umschalten (`- [ ]` ↔ `- [x]`, macht aus einer Zeile einen Punkt) | `Strg+Umschalt+X` | .md |
| Bearbeiten ↔ Vorschau | `Strg+E` | .md |
| Zen-Modus | `Strg+Umschalt+F` | beide |
| Umbenennen | `F2` (im Baum) bzw. Klick auf den Namen in der Statuszeile | beide |
| Modus wechseln | `Strg+1` / `Strg+2` | global |
| Befehlspalette | `Strg+K` | global (bleibt) |

Die Kürzel kommen in die Kürzel-Registry (Gruppe „Notizen"), in `?` und in die Tooltips. Für das Web (N1) ist zu
prüfen, ob der Browser `Strg+L` (Adressleiste) abgeben lässt. Sonst bekommt das Web eine Ausweichbelegung.

### 6.5 Markdown-Vorschau

- **Eigener Renderer** in `features/notes/markdown/` (Ziel ≤ 15 KB gzip, liegt im Notes-Chunk): CommonMark-Teilmenge
  (ATX-Überschriften, Absätze, harte Umbrüche, `**`/`*`/`_`, Inline-Code, Codeblöcke ``` und eingerückt, Zitate,
  verschachtelte Listen, Trennlinie, Links `[t](u)` und `<u>`) plus GFM: Checklisten, Tabellen (mit Ausrichtung),
  `~~` und Auto-Links. Bewusst nicht: Raw-HTML, Bilder, Fußnoten, Referenz-Links, Setext-Überschriften. Diese Teile
  bleiben als Text sichtbar.
- **Sicherheit:** Der Renderer erzeugt DOM-Knoten bzw. escaped HTML, **nie** ungeprüftes `innerHTML` aus dem Text.
  Links nur `http:`, `https:`, `mailto:`. Ein Klick öffnet sie im System-Browser (Shell), nie in der WebView.
  Tests mit XSS-Fällen sind Pflicht.
- **Checklisten** in der Vorschau sind anklickbar. Der Renderer merkt sich je Checkbox die Quellzeile. Ein Klick
  ändert genau `[ ]` ↔ `[x]` in dieser Zeile und speichert wie eine Eingabe.
- Vorschau scrollt zur Position, die dem Cursor im Quelltext entspricht (über Zeilenanker), und zurück.
- **Budget:** 20 KB Markdown rendern < 8 ms, 1 MiB < 150 ms (dann einmal „Vorschau wird erstellt…").
  Gerendert wird nur beim Umschalten bzw. bei Änderungen, nie dauerhaft im Hintergrund.
- `.txt`-Notizen haben keine Vorschau.

### 6.6 Einstellungen (Abschnitt „Notizen" im bestehenden Dialog, pro Gerät)

- Standardtyp neuer Notizen: **Markdown (.md)** / Text (.txt).
- Zeilenbreite begrenzen: an/aus.
- Später: Rechtschreibprüfung an/aus (v1 aus).

## 7. Suche (gemeinsam mit Tasks)

- Neue Port-Methode `NoteRepository::search_notes(&SearchQuery)`, gleiche Semantik wie `search_tasks` (ADR-027): jedes
  Wort als Teilwort in Name **oder** Inhalt. Rang: Namenstreffer vor Inhaltstreffern, dann zuletzt geändert.
  Elemente im Papierkorb nie. SQLite über `notes_fts` (Trigram), Standard-Implementierung O(n) für In-Memory/Web.
- Die **Befehlspalette** (`Strg+K`) fragt beide parallel ab und zeigt die Gruppen „Tasks" und „Notizen" (Icon,
  Ordnerpfad, bei Inhaltstreffern ein Ausschnitt von ±40 Zeichen um den Treffer). `Enter` auf einer Notiz wechselt in
  den Notes-Modus und öffnet sie. Der Cursor springt zum ersten Treffer.
- Budget wie ADR-027: p95 < 16 ms bei 2 000 Notizen à 20 KB (Messung in N5-10).

## 8. Import (nur Desktop)

- „Ordner importieren…" (`⋯` in der Seitenleiste, Palette). Die Ordnerauswahl kommt über das XDG-Desktop-Portal bzw.
  `tauri-plugin-dialog`. Größe und Abhängigkeiten prüft N5-11 vor der Wahl.
- **Einmalige Kopie:** Unterordner werden zu Notiz-Ordnern unter einem neuen Ordner „<Ordnername>" in der Wurzel.
  `.md`/`.markdown` werden `Markdown`, `.txt` wird `Text`. Name = Dateiname ohne Endung, `auto_name = false`.
- Übersprungen und im Bericht aufgeführt werden: versteckte Dateien und Ordner (`.obsidian`, `.git` …), andere
  Endungen, Nicht-UTF-8, > 1 MiB, Symlinks (nicht folgen), Tiefe > 32.
- Zeilenenden werden auf LF normalisiert, ein UTF-8-BOM wird entfernt.
- Die Shell liest die Dateien, der Core importiert über einen Use-Case `ImportNotes` in Commits zu je ≤ 500 Elementen
  (ein Upload-Auslöser je Commit). Der Quellordner wird nie verändert.
- Ergebnis-Dialog: „412 Notizen in 37 Ordnern importiert, 9 übersprungen" mit aufklappbarer Liste der Gründe.

## 9. Performance-Budgets (Ergänzung zu `AGENTS.md` §4)

| Metrik | Ziel |
|---|---|
| Initiales JS (Tasks-Start) | **+0 KB** außer Umschalter. Notes ist ein eigener Lazy-Chunk. |
| Notes-Chunk (Baum + Editor + Renderer, gzip) | < 25 KB |
| Wechsel Tasks → Notes, erster (Chunk laden + Baum) / weitere | < 150 ms / < 50 ms |
| Notiz öffnen (20 KB) bis Text sichtbar | < 50 ms |
| Tastendruck bis Repaint im Editor | < 16 ms (natives `textarea`, keine Arbeit je Taste außer Timer-Reset) |
| Autosave-Commit (20 KB) | < 10 ms im Core |
| Ordner mit 500 Einträgen aufklappen | < 30 ms |
| Idle-RAM-Zuwachs mit offener Notiz | ≤ 10 MB PSS (Messung `tools/measure-desktop.sh` im Notes-Modus) |
| Idle-CPU | ~0 %: Timer laufen nur nach Eingaben, Auto-Leeren nur beim Start bzw. täglich, kein Polling |

## 10. Module (Vorschlag)

```
crates/todo-core/src/domain/note.rs, note_folder.rs, note_tree.rs, note_name.rs
crates/todo-core/src/sync/revision.rs            # RevisionSet, heads, lca, merge3, resolve_body
crates/todo-core/src/ports/note_repository.rs
crates/todo-core/src/app/create_note.rs, update_note_body.rs, rename_note.rs, set_note_kind.rs, move_note.rs,
    create_note_folder.rs, rename_note_folder.rs, move_note_folder.rs, trash_notes.rs, restore_notes.rs,
    purge_notes.rs, empty_note_trash.rs, purge_expired_notes.rs, resolve_note_heads.rs, import_notes.rs
crates/todo-storage-sqlite/migrations/00NN_notes.sql
packages/ui/src/lib/app-mode.svelte.ts            # Modus + Persistenz
packages/ui/src/features/notes/
├── NotesSidebar.svelte    NoteTree.svelte    NoteTreeRow.svelte    tree-model.ts (+ test)
├── NoteEditor.svelte      editor-commands.ts (Listen, Format; + test)    stats.ts
├── NoteStatusBar.svelte   NotesEmpty.svelte   NoteTrash.svelte
└── markdown/ render.ts (+ test, inkl. XSS)  MarkdownPreview.svelte
```

## 11. Offene Punkte und Später

Die kleinen Punkte der ersten Fassung (Rechtschreibprüfung, Überschriften-Kürzel, Upload-Timing, leere Notiz) sind
geklärt (§0).

1. **Domain-Sprache:** `Note`, `NoteFolder` kommen in die verbindliche Liste in `AGENTS.md` §5. „Notes" ist auch
   UI-Begriff (Modus-Name), alle anderen UI-Texte bleiben Deutsch („Neue Notiz", „Ordner").
2. **Später:** Export (einzeln/alles), `[[Verknüpfungen]]` über das `Link`-Element (N3), Tags, Versionsverlauf aus
   den Revisionen, Delta-Revisionen, Mathe (KaTeX lazy), Anhänge als eigene verschlüsselte Dateien.
