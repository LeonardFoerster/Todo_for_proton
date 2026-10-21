-- 0008: Tags als eigene Entity (N8-02, ADR-035). entity_kind 4 = Tag.
CREATE TABLE tags (
    id          TEXT    NOT NULL PRIMARY KEY,
    name        TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- Register und Tombstones für die neue Entity-Art (CHECK lässt sich nur durch Neuanlage ändern).
CREATE TABLE field_clocks_new (
    entity_id    TEXT    NOT NULL,
    field        INTEGER NOT NULL,
    entity_kind  INTEGER NOT NULL CHECK (entity_kind IN (0, 1, 2, 3, 4)),
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
    entity_kind  INTEGER NOT NULL CHECK (entity_kind IN (0, 1, 2, 3, 4)),
    hlc          BLOB    NOT NULL
) STRICT, WITHOUT ROWID;
INSERT INTO tombstones_new SELECT entity_id, entity_kind, hlc FROM tombstones;
DROP TABLE tombstones;
ALTER TABLE tombstones_new RENAME TO tombstones;
