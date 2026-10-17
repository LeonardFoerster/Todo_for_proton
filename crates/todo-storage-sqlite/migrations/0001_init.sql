-- 0001: Listen, Tasks, Metadaten.
-- IDs: UUID-Strings (lowercase, hyphenated) – deren Byte-Reihenfolge entspricht der von `TaskId`.
-- Zeitpunkte: Unix-Millisekunden UTC. Sort-Keys: Fractional Index, Vergleich per BINARY-Collation.

CREATE TABLE task_lists (
    id          TEXT    NOT NULL PRIMARY KEY,
    name        TEXT    NOT NULL,
    sort_key    TEXT    NOT NULL COLLATE BINARY,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    is_inbox    INTEGER NOT NULL CHECK (is_inbox IN (0, 1))
) STRICT;

CREATE TABLE tasks (
    id            TEXT    NOT NULL PRIMARY KEY,
    -- Aufgeschoben: geprüft wird erst beim COMMIT, damit ein ChangeSet Tasks verschieben und
    -- danach ihre alte Liste löschen kann. Verstöße lassen die ganze Transaktion scheitern.
    list_id       TEXT    NOT NULL REFERENCES task_lists (id) DEFERRABLE INITIALLY DEFERRED,
    parent_id     TEXT,
    title         TEXT    NOT NULL,
    notes         TEXT    NOT NULL,
    status        INTEGER NOT NULL CHECK (status IN (0, 1)),          -- 0 = offen, 1 = erledigt
    priority      INTEGER NOT NULL CHECK (priority BETWEEN 0 AND 3),  -- None, Low, Medium, High
    -- Fälligkeit: entweder ein Datum (Tage seit 0001-01-01, proleptisch gregorianisch) …
    due_date      INTEGER,
    -- … oder ein Zeitpunkt mit (nur gespeicherter) Zeitzone.
    due_at        INTEGER,
    due_tz        TEXT,
    -- Tag-IDs, durch Leerzeichen getrennt. Eigene Tabelle, sobald Tags Logik bekommen.
    tags          TEXT    NOT NULL,
    sort_key      TEXT    NOT NULL COLLATE BINARY,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    completed_at  INTEGER,
    CHECK (due_date IS NULL OR due_at IS NULL),
    CHECK ((due_at IS NULL) = (due_tz IS NULL)),
    CHECK ((status = 1) = (completed_at IS NOT NULL))
) STRICT;

-- Listenansicht (List/Inbox): Filter auf list_id, sortiert nach sort_key, id.
CREATE INDEX tasks_by_list ON tasks (list_id, sort_key, id);
-- „Alle": sortiert über alle Listen.
CREATE INDEX tasks_by_sort_key ON tasks (sort_key, id);
-- Heute/Demnächst.
CREATE INDEX tasks_by_status_due_date ON tasks (status, due_date);
CREATE INDEX tasks_by_status_due_at ON tasks (status, due_at);
-- Erledigt: neueste zuerst.
CREATE INDEX tasks_by_status_completed ON tasks (status, completed_at);

-- Kleine Schlüssel/Wert-Ablage (z. B. Geräte-ID ab P5).
CREATE TABLE meta (
    key    TEXT NOT NULL PRIMARY KEY,
    value  TEXT NOT NULL
) STRICT, WITHOUT ROWID;
