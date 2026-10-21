-- 0009: Verknüpfungen Task ↔ Notiz (N9, ADR-036). entity_kind 5 = Verknüpfung. Keine Fremdschlüssel:
-- ein Ende kann per Sync fehlen (die UI blendet solche Verknüpfungen aus, das Aufräumen löscht sie).
CREATE TABLE links (
    id          TEXT    NOT NULL PRIMARY KEY,
    task_id     TEXT    NOT NULL,
    note_id     TEXT    NOT NULL,
    created_at  INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
CREATE INDEX links_by_task ON links (task_id);
CREATE INDEX links_by_note ON links (note_id);

CREATE TABLE field_clocks_new (
    entity_id    TEXT    NOT NULL,
    field        INTEGER NOT NULL,
    entity_kind  INTEGER NOT NULL CHECK (entity_kind IN (0, 1, 2, 3, 4, 5)),
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
    entity_kind  INTEGER NOT NULL CHECK (entity_kind IN (0, 1, 2, 3, 4, 5)),
    hlc          BLOB    NOT NULL
) STRICT, WITHOUT ROWID;
INSERT INTO tombstones_new SELECT entity_id, entity_kind, hlc FROM tombstones;
DROP TABLE tombstones;
ALTER TABLE tombstones_new RENAME TO tombstones;
