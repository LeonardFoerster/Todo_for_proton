-- 0004: Notizen und Ordner (ADR-029, `development/notes.md` §3.5).
-- entity_kind: 0 = Task, 1 = Liste, 2 = Notiz, 3 = Notiz-Ordner.

-- Ordner; parent_id = wirksamer Elternordner (Waisen und Zyklen sind schon aufgelöst).
CREATE TABLE note_folders (
    id          TEXT    NOT NULL PRIMARY KEY,
    parent_id   TEXT    REFERENCES note_folders (id) DEFERRABLE INITIALLY DEFERRED,
    name        TEXT    NOT NULL,
    sort_key    TEXT    NOT NULL COLLATE BINARY,
    created_at  INTEGER NOT NULL,
    trashed_at  INTEGER
) STRICT;

CREATE INDEX note_folders_by_parent ON note_folders (parent_id, sort_key, id);

-- Notizen; body = aufgelöster Inhalt aller Revisionen, body_rev = "<gerät>.<nummer>".
-- body steht zuletzt: Große Inhalte liegen in Überlaufseiten, und Baum, Papierkorb und Suche lesen
-- nur die Spalten davor, ohne diese Seiten zu laden.
CREATE TABLE notes (
    id           TEXT    NOT NULL PRIMARY KEY,
    folder_id    TEXT    REFERENCES note_folders (id) DEFERRABLE INITIALLY DEFERRED,
    name         TEXT    NOT NULL,
    auto_name    INTEGER NOT NULL CHECK (auto_name IN (0, 1)),
    kind         INTEGER NOT NULL CHECK (kind IN (0, 1)),          -- 0 = .txt, 1 = .md
    body_rev     TEXT,
    size         INTEGER NOT NULL,                                 -- Bytes von body
    sort_key     TEXT    NOT NULL COLLATE BINARY,
    created_at   INTEGER NOT NULL,
    modified_at  INTEGER NOT NULL,
    trashed_at   INTEGER,
    body         TEXT    NOT NULL
) STRICT;

CREATE INDEX notes_by_folder ON notes (folder_id, sort_key, id);
CREATE INDEX notes_by_trash ON notes (trashed_at) WHERE trashed_at IS NOT NULL;

-- Revisionen des Inhalts, solange sie Kopf oder mögliche Basis sind (§4.5).
-- parents = postcard(Vec<OpId>), hlc = 28 Bytes.
CREATE TABLE note_revisions (
    note_id     TEXT    NOT NULL,
    rev_device  TEXT    NOT NULL,
    rev_seq     INTEGER NOT NULL,
    hlc         BLOB    NOT NULL,
    parents     BLOB    NOT NULL,
    text        TEXT    NOT NULL,
    PRIMARY KEY (note_id, rev_device, rev_seq)
) STRICT;

-- Eltern je Revision als Kanten: Köpfe (= nie Elternteil) lassen sich so ohne die Texte älterer
-- Revisionen finden (jeder Autosave fragt danach).
CREATE TABLE note_revision_parents (
    note_id        TEXT    NOT NULL,
    parent_device  TEXT    NOT NULL,
    parent_seq     INTEGER NOT NULL,
    rev_device     TEXT    NOT NULL,
    rev_seq        INTEGER NOT NULL,
    PRIMARY KEY (note_id, parent_device, parent_seq, rev_device, rev_seq)
) STRICT, WITHOUT ROWID;

CREATE INDEX note_revision_parents_by_child ON note_revision_parents (note_id, rev_device, rev_seq);

-- Outbox: Entity und „Platz" der Op, damit eine noch nicht herausgegebene Op derselben Notiz
-- gefunden und ersetzt werden kann (§4.6). slot: -1 = Revise, 0 = Create, sonst Feld-Code eines
-- Set; NULL bei Delete und bei Ops von vor dieser Migration.
ALTER TABLE outbox_ops ADD COLUMN entity_id TEXT;
ALTER TABLE outbox_ops ADD COLUMN slot INTEGER;
CREATE INDEX outbox_by_entity ON outbox_ops (entity_id, slot, seq) WHERE entity_id IS NOT NULL;

-- Register und Tombstones für die neuen Entity-Arten (CHECK lässt sich nur durch Neuanlage ändern).
CREATE TABLE field_clocks_new (
    entity_id    TEXT    NOT NULL,
    field        INTEGER NOT NULL,
    entity_kind  INTEGER NOT NULL CHECK (entity_kind IN (0, 1, 2, 3)),
    hlc          BLOB    NOT NULL,
    value        BLOB    NOT NULL,
    PRIMARY KEY (entity_id, field)
) STRICT, WITHOUT ROWID;
INSERT INTO field_clocks_new SELECT entity_id, field, entity_kind, hlc, value FROM field_clocks;
DROP TABLE field_clocks;
ALTER TABLE field_clocks_new RENAME TO field_clocks;
CREATE INDEX field_clocks_by_kind ON field_clocks (entity_kind, entity_id);

CREATE TABLE tombstones_new (
    entity_id    TEXT    NOT NULL PRIMARY KEY,
    entity_kind  INTEGER NOT NULL CHECK (entity_kind IN (0, 1, 2, 3)),
    hlc          BLOB    NOT NULL
) STRICT, WITHOUT ROWID;
INSERT INTO tombstones_new SELECT entity_id, entity_kind, hlc FROM tombstones;
DROP TABLE tombstones;
ALTER TABLE tombstones_new RENAME TO tombstones;

-- Suche über Name und Inhalt (`notes.md` §7, wie 0003 für Tasks): Trigramme, ohne Groß/Klein,
-- externer Inhalt. Nur echte Text-Änderungen indizieren (Autosave schreibt die ganze Zeile).
CREATE VIRTUAL TABLE notes_fts USING fts5 (
    name,
    body,
    content = 'notes',
    content_rowid = 'rowid',
    tokenize = 'trigram case_sensitive 0'
);

CREATE TRIGGER notes_fts_insert AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts (rowid, name, body) VALUES (new.rowid, new.name, new.body);
END;

CREATE TRIGGER notes_fts_delete AFTER DELETE ON notes BEGIN
    INSERT INTO notes_fts (notes_fts, rowid, name, body) VALUES ('delete', old.rowid, old.name, old.body);
END;

CREATE TRIGGER notes_fts_update AFTER UPDATE OF name, body ON notes
WHEN old.name IS NOT new.name OR old.body IS NOT new.body BEGIN
    INSERT INTO notes_fts (notes_fts, rowid, name, body) VALUES ('delete', old.rowid, old.name, old.body);
    INSERT INTO notes_fts (rowid, name, body) VALUES (new.rowid, new.name, new.body);
END;
