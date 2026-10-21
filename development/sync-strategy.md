# Offline-First Sync mit Proton Drive

## 1. Grundsätze

1. **Lokal ist die Wahrheit für die UI.** Jede Aktion wird sofort lokal committed; Sync ist asynchron.
2. **Drive ist nur ein Dateispeicher.** Keine Transaktionen, keine Locks, Latenz im Sekundenbereich,
   Rate-Limits. Das Format muss damit robust umgehen.
3. **Jedes Gerät schreibt nur eigene Dateien.** Dadurch gibt es keine Schreibkonflikte auf Drive.
   Inhaltliche Konflikte löst der Core deterministisch.
4. **Konvergenz:** Alle Geräte, die dieselbe Menge an Ops gesehen haben, haben denselben Zustand –
   unabhängig von der Reihenfolge des Empfangs.

## 2. Datenmodell für Sync

### Ops (feldgranular)
```rust
pub struct Op {
    pub id: OpId,                 // (device_id, seq) – eindeutig, monoton pro Gerät
    pub hlc: Hlc,                 // Hybrid Logical Clock: (wall_ms, counter, device_id)
    pub entity: EntityRef,        // Task(TaskId) | TaskList(..)   (Tag später)
    pub kind: OpKind,
}
pub enum OpKind {
    Create { fields: FieldMap },          // initiale Feldwerte
    Set    { field: FieldName, value: FieldValue },
    Delete,                               // Tombstone
    // später (Tags als OR-Set): AddToSet / RemoveFromSet – im Prototyp nicht umgesetzt
}
```

### Merge-Regeln
- **Feldweise Last-Writer-Wins (LWW-Register)** per HLC; Gleichstand → höhere `device_id` gewinnt.
  Beispiel: Gerät A ändert offline den Titel, Gerät B das Fälligkeitsdatum → beide Änderungen bleiben erhalten.
- **Delete** ist ein Tombstone mit HLC. Ein `Set` mit *höherer* HLC nach dem Delete wird ignoriert
  (Delete gewinnt – „Delete-wins" ist für Todos die erwartete Semantik). Wiederherstellen = expliziter `Restore`-Op (später).
- **Status Done/Open**: normales LWW-Feld; `completed_at` wird mitgeführt.
- **Wiederholung (N4, ADR-028)**: Feld `Recurrence` (Code 17, ganze Regel als LWW-Register, `None` = keine).
  Abhaken einer Serie = ein `ChangeSet` aus „Serie: neue `due`" und „Verlaufseintrag anlegen". Die ID des Eintrags
  ist **deterministisch** (UUIDv5 aus Serie + lokalem Tag des Vorkommens): Haken zwei Geräte offline dasselbe
  Vorkommen ab, entsteht nur **ein** Eintrag (gleiche ID, Felder per LWW), und beide schreiben dieselbe `due`. Nach
  „Rückgängig" (Tombstone) nimmt erneutes Abhaken die nächste ID-Variante. Ältere Apps kennen Code 17 nicht und
  überspringen solche Segmente (Hinweis „alle Geräte aktualisieren").
- **Notizen**: v1 LWW für das ganze Feld. Später ggf. Text-CRDT (z. B. `yrs`) nur für `notes`.
- **Checkliste (N8, ADR-034)**: Feld `Checklist` (Code 18), LWW auf die ganze Liste der Punkte `{id, text, done}`.
- **Tags**: v1 LWW auf die ganze Menge; bei Bedarf Upgrade auf OR-Set über `AddToSet`/`RemoveFromSet`.
- **Reihenfolge**: `sort_key` als Fractional Index (String, z. B. Base62). Verschieben ändert nur
  das eine Feld der bewegten Task. Gleiche Keys nach parallelen Moves → Tie-Break über `TaskId`.
  Gelegentliches Rebalancing nur lokal bei zu langen Keys (erzeugt normale `Set`-Ops).
- **Hierarchie**: Subtask, deren Parent gelöscht wurde → wird mit gelöscht (Kaskade beim Materialisieren,
  nicht als eigene Ops).

### Notes (N5, umgesetzt 2026-10-20 – `notes.md` §4, ADR-029)
- `EntityRef::Note`/`NoteFolder`, Felder 13–16 und `FieldValue::Folder`/`NoteKind`/`Trashed` angehängt (postcard,
  Golden-Dateien `ops_v1_notes.bin`). Name, Ordner, Reihenfolge, Typ und Papierkorb sind LWW-Felder; der Name wird
  nicht synchronisiert, solange er automatisch aus der ersten Zeile folgt (jedes Gerät leitet ihn aus dem
  aufgelösten Inhalt ab).
- Der Inhalt ist **kein** LWW-Feld, sondern ein DAG unveränderlicher Revisionen (`OpKind::Revise { parents, text }`,
  `RevisionId` = `OpId`, höchstens 64 Eltern). Mehrere Köpfe werden in `apply_remote` deterministisch aufgelöst:
  Merge-Basis (bei Kreuz-Merges virtuelle Basis), eigener Myers-Diff + diff3 (`sync/text_merge.rs`; benachbarte
  Zeilen sind kein Konflikt). Überlappung → Konfliktkopie mit UUIDv5-ID aus den beteiligten Revisionen und Name
  „<Name> (Konflikt TT.MM.JJJJ)"; jedes Gerät erzeugt dieselbe Kopie, nie doppelt. Die Merge-Revision entsteht im
  selben Commit bzw. Pull (Property-Test: Konvergenz bei beliebiger Reihenfolge).
- Ordnerbaum: verwaiste Einträge → oberste Ebene, Zyklen bricht der `ParentFolder`-Wert mit der höchsten HLC,
  Papierkorb wird vererbt, Tiefe ≤ 32. Papierkorb = LWW-Feld `Trashed`, endgültig = `Delete` (nach 30 Tagen
  automatisch, täglich beim Start).
- Outbox: ungepushte `Revise`/`Set`-Ops derselben Notiz werden zusammengefasst (gleiche `seq`, keine Lücken im
  Segment); vor dem Upload friert `freeze_outbox` den Stand ein. Segmente bis 2 MiB Klartext.
- Snapshot-Format v2 enthält Notizen, Ordner und die gespeicherten Revisionen (`Revise`-Ops mit Original-ID).
  Compaction kürzt vorher: Revisionen, die alle Geräte bestätigt haben und die kein Kopf mehr braucht, entfallen.
- Upload für Inhalte: Scheduler-Ereignis `ContentChange` (Debounce 3 s, höchstens 30 s); sofort (`Flush`) bei
  Notiz-/Moduswechsel, Fokusverlust und Schließen.
- **Kompatibilität:** Geräte mit älterer App-Version können die neuen Op-Varianten nicht lesen – alle Geräte
  gleichzeitig aktualisieren (README).

### Lokale Tabellen (Auszug)
| Tabelle | Zweck |
|---|---|
| `tasks`, `task_lists` | materialisierter Zustand (für schnelle Queries); `tags` später |
| `tombstones` | gelöschte Entities mit HLC (Delete-wins) |
| `meta` | Schema-/Speicher-Metadaten |
| `field_clocks` | `(entity_id, field) → hlc` – nötig für LWW beim Mergen |
| `outbox_ops` | lokal erzeugte, noch nicht hochgeladene Ops |
| `remote_cursors` | pro fremdem Gerät: zuletzt verarbeitetes Segment |
| `sync_meta` | eigene `device_id`, nächste `seq`, letzter Snapshot, HLC-Zustand |
| `notes`, `note_folders` | materialisierte Notizen (Inhalt = aufgelöster Text) und Ordner (Migration 0004) |
| `note_revisions`, `note_revision_parents` | Revisions-DAG (Text je Revision, Kanten); geladen werden nur die Köpfe |
| `notes_fts` | Volltext (FTS5, Trigramm) für die Suche über Namen und Inhalt |

## 3. Layout im Proton Drive

```
<App-Ordner>/                          "Tasks" unter „Meine Dateien" (SyncConfig::app_folder)
├── manifest.json                      { format_version, created_hlc, app_id }
├── keys/sync-key.enc                  App-Sync-Schlüssel (siehe interfaces.md §5)
├── devices/<device_id>.json           { name, platform, last_seen, last_seq }   ← nur Besitzer schreibt
├── ops/<device_id>/<seq_from>-<seq_to>.seg   unveränderliche Segmente (zstd + XChaCha20)
└── snapshots/<hlc>-<device_id>.snap   kompletter Zustand + Vektor der eingeschlossenen Seqs
```

**Segment** = Header (`format_version`, `device_id`, `seq_from..seq_to`) + zstd-komprimierte, verschlüsselte
Op-Liste (postcard, ADR-016). Ein Segment wird nie verändert, nur irgendwann gelöscht (Kompaktierung).

## 4. Ablauf

### Push
1. Trigger: Outbox nicht leer **und** (Debounce 1,5 s seit letzter Änderung, **spätestens 5 s nach der ersten Änderung einer Serie** (ADR-022, P1-02) **oder** App geht in Hintergrund/wird geschlossen
   **oder** manueller Sync).
2. Bis zu N Ops (z. B. 500) aus Outbox → Segment bauen → `RemoteStore::create`.
3. Erfolg → `mark_ops_pushed`, `devices/<id>.json` gelegentlich aktualisieren (nicht bei jedem Push).
4. `Conflict` bei `create` (Datei existiert schon, z. B. Absturz nach Upload) → Inhalt vergleichen, als gepusht markieren.

### Pull
1. Trigger: App-Start (nach erstem Render!), Fenster-Fokus, Netz wieder da, nach eigenem Push,
   periodisch nur solange Fenster sichtbar (z. B. 60 s → mit Backoff bis 10 min). Wenn verfügbar: Drive-Event-Feed
   (`changes_since`) statt Listing.
2. `ops/*/` listen → Segmente mit `seq > remote_cursor[device]` laden (parallel, max. 4).
3. Entschlüsseln, dekomprimieren, Ops nach HLC sortieren → `merge` gegen `field_clocks` →
   `apply_remote` in einer Transaktion → Cursor setzen → `ChangeEvent`.
4. Lokale HLC mit empfangenen HLCs vorrücken (HLC-Regel).

Umsetzung (P5-08, `todo_sync::engine`): Mit Änderungs-Feed werden nur die Ordner `ops/<device>/` gelistet, in denen
der Feed Änderungen meldet (im Ruhezustand: kein Listing); ohne Feed bzw. bei Feed-Lücke (`refresh`) einmal alles.
Der Cursor eines Geräts rückt nur über eine **lückenlose** Kette gelesener Segmente vor; Ops hinter einer Lücke oder
einem kaputten Segment werden trotzdem angewendet (idempotent) und später erneut gelesen. Bis zu 20 Segmente bilden
ein `apply_remote` (eine Transaktion, ein Event). `devices/<id>.json` (inkl. `cursors`) wird beim ersten Durchlauf
und danach höchstens alle 10 min geschrieben. Das Manifest wird zuerst nur auf `format_version` gelesen.

### Erstes Gerät / neues Gerät
- **Erstes Gerät:** App-Ordner + `manifest.json` + Sync-Schlüssel anlegen, lokale Daten als Initial-Ops pushen.
- **Neues Gerät:** neuesten Snapshot laden → als Basis importieren → nur Segmente nach dem Snapshot-Vektor nachladen.
- **Lokale Daten vor erstem Login:** werden beim Verbinden als normale Ops gepusht (keine Duplikate, da UUIDs).
  Die Inbox hat überall dieselbe feste ID und verschmilzt; gleichnamige Listen bleiben getrennt (ADR-020).

### Kompaktierung
- Ein Gerät erstellt einen Snapshot, wenn > 2 000 Ops seit letztem Snapshot oder > 7 Tage.
  Snapshots verschiedener Geräte kollidieren nicht (eindeutige Namen); der neueste gültige gewinnt.
- Ein Gerät löscht **nur eigene** Segmente, und nur wenn sie in einem Snapshot enthalten sind, dessen
  Vektor alle aktiven Geräte abdeckt (aktiv = `last_seen` < 90 Tage).
- Umsetzung (P5-11): Snapshot = alle Register/Tombstones + Vektor, geschrieben nur bei leerer Outbox. Importiert wird
  als State-Join über `apply_remote` (Register → `Set` mit ihrer HLC, Tombstone → `Delete`) – beim ersten Pull eines
  neuen Geräts und wenn am Anfang einer Segmentkette eine Lücke ist (kompaktiert). Gelöscht wird nur, was der
  **vorletzte** eigene Snapshot abdeckt (Ersatz bei kaputtem neuestem), und nie das jüngste eigene Segment (sonst
  erkennt ein zurückliegendes Gerät die Lücke nicht). Je Gerät bleiben 2 eigene Snapshots liegen.
- Tombstones werden im Snapshot nach 90 Tagen entfernt (noch offen). Geräte, die länger offline waren, machen beim
  nächsten Start einen **Full-Resync** vom Snapshot (lokale ungepushte Ops werden danach erneut angewendet).

## 5. Fehler- und Randfälle

| Fall | Verhalten |
|---|---|
| Kein Netz / Timeout | Ops bleiben in der Outbox; Status „Offline"; Retry mit exponentiellem Backoff + Jitter (5 s → 10 min) |
| HTTP 429 / Rate-Limit | `Retry-After` respektieren, Scheduler pausiert global |
| Session abgelaufen | `AuthProvider::resume`; schlägt das fehl → Status „Anmeldung erforderlich", Sync pausiert, App bleibt nutzbar |
| Segment beschädigt / Entschlüsselung schlägt fehl | Segment überspringen, Fehler loggen (ohne Inhalt), Status-Warnung; nicht den ganzen Sync abbrechen |
| Unbekannte `format_version` (neuere App) | Manifest: Sync pausiert, Hinweis „App aktualisieren"; einzelnes Segment/Snapshot: nur überspringen (Version steht vor der Authentisierung, ADR-025); nie fremde Formate überschreiben |
| Systemuhr falsch | HLC begrenzt den Schaden; Ops mit Wall-Clock > jetzt + 24 h werden mit Warnung akzeptiert, aber HLC nicht blind übernommen; Ops nach 2100-01-01 (`MAX_WALL_MS`) werden überall verworfen |
| Datei am falschen Ort (Header ≠ Pfad) | Segment/Snapshot wie beschädigt behandeln: Cursor rückt nicht vor (ADR-025) |
| Lokaler Sync-Schlüssel ≠ `keys/sync-key.enc`, oder Schlüsseldatei fehlt in einem Ordner mit Daten | Fehler statt neuem Schlüssel; „ab- und wieder anmelden" (ADR-025) |
| Absturz zwischen Upload und `mark_ops_pushed` | Idempotent: Op-IDs sind eindeutig, doppelte Anwendung ist No-Op |

## 6. Sync-Status für die UI

```rust
pub enum SyncStatus {
    Disabled,                       // nicht angemeldet
    Idle { last_success: Option<Timestamp> },
    Syncing,
    Offline { pending_ops: u64 },
    Paused(SyncPauseReason),        // AuthRequired | RateLimited | NewerFormat
    Error { message: String },
}
```
Darstellung dezent (kleines Icon in der Sidebar-Fußzeile, wie Protons Speicher-/Status-Anzeige), keine Modals.

## 7. Tests

- **Property-Test Konvergenz:** zufällige Op-Folgen auf 2–4 simulierten Geräten, zufällige Liefer-Reihenfolge,
  Duplikate → alle Endzustände identisch.
- **Simulation** mit `InMemoryRemoteStore` + Fehler-Injektion (Timeouts, 429, abgebrochene Uploads).
- **Golden Files** für Segment-/Snapshot-Format (Formatstabilität über Versionen).
- **Ende-zu-Ende** gegen `FsRemoteStore` mit zwei App-Instanzen auf demselben Verzeichnis.

## 8. Performance-Hinweise
- Segmente bündeln (nicht ein Upload pro Tastendruck); Debounce + Batch.
- Auslöser eines Uploads sind genau die schreibenden Commands (Task anlegen/ändern/abhaken/verschieben/löschen, deren
  Sammel-Varianten, Listen anlegen/umbenennen/löschen) – je Aufruf eine Meldung an den Scheduler; Liste und Nachweis in
  `apps/desktop/src-tauri/src/commands.rs` bzw. `tests/commands.rs`. Eine Serie (z. B. mehrere Tasks hintereinander
  abhaken) ergibt einen Upload, spätestens 5 s nach ihrem Beginn.
- Pull nur Deltas (Cursor); Snapshot nur beim Bootstrap.
- Merge in einer einzigen SQLite-Transaktion; danach **ein** `ChangeEvent` mit allen geänderten IDs.
- Sync läuft auf Desktop in einem Tokio-Task, im Web im Worker – nie auf dem UI-Thread.
- Kein Sync-Polling bei minimiertem/verstecktem Fenster.
