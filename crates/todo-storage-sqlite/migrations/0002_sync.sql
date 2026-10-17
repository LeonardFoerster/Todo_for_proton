-- 0002: Sync-Buchhaltung (ADR-015). Binärwerte im kanonischen postcard-Format (ADR-016):
-- HLC = 28 Bytes (memcmp-sortierbar), Feldwerte und Ops = postcard.
-- entity_kind: 0 = Task, 1 = Liste.

-- Eigenes Gerät, HLC-Zustand, letzte Op-Nummer; außerdem Werte der Sync-Engine (Präfix „engine.").
CREATE TABLE sync_meta (
    key    TEXT NOT NULL PRIMARY KEY,
    value  ANY  NOT NULL
) STRICT, WITHOUT ROWID;

-- Lokal erzeugte, noch nicht hochgeladene Ops. seq = Op-Nummer dieses Geräts.
CREATE TABLE outbox_ops (
    seq  INTEGER NOT NULL PRIMARY KEY,
    op   BLOB    NOT NULL
) STRICT;

-- LWW-Register pro Entity und Feld (Wert + HLC der gewinnenden Op).
CREATE TABLE field_clocks (
    entity_id    TEXT    NOT NULL,
    field        INTEGER NOT NULL,
    entity_kind  INTEGER NOT NULL CHECK (entity_kind IN (0, 1)),
    hlc          BLOB    NOT NULL,
    value        BLOB    NOT NULL,
    PRIMARY KEY (entity_id, field)
) STRICT, WITHOUT ROWID;

-- Neuzuordnung nach Listenänderungen liest alle Task-Register.
CREATE INDEX field_clocks_by_kind ON field_clocks (entity_kind, entity_id);

-- Gelöschte Entities. Gewinnen gegen alle Ops (Delete-wins).
CREATE TABLE tombstones (
    entity_id    TEXT    NOT NULL PRIMARY KEY,
    entity_kind  INTEGER NOT NULL CHECK (entity_kind IN (0, 1)),
    hlc          BLOB    NOT NULL
) STRICT, WITHOUT ROWID;

-- Pro fremdem Gerät: Nummer der zuletzt verarbeiteten Op.
CREATE TABLE remote_cursors (
    device_id  TEXT    NOT NULL PRIMARY KEY,
    seq        INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
