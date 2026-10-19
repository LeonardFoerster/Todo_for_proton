-- 0003: Volltextsuche über Titel und Notizen (P4-05, ADR-027).
-- FTS5 mit Trigram-Tokenizer: findet Teilwörter („richt“ in „Bericht“), ohne Groß/Klein. Externer
-- Inhalt (`content = 'tasks'`): der Index hält nur Trigramme, den Text liefert `tasks`; Trigger
-- halten ihn bei Einfügen, Ändern (UPSERT = UPDATE) und Löschen aktuell.

CREATE VIRTUAL TABLE tasks_fts USING fts5 (
    title,
    notes,
    content = 'tasks',
    content_rowid = 'rowid',
    tokenize = 'trigram case_sensitive 0'
);

CREATE TRIGGER tasks_fts_insert AFTER INSERT ON tasks BEGIN
    INSERT INTO tasks_fts (rowid, title, notes) VALUES (new.rowid, new.title, new.notes);
END;

CREATE TRIGGER tasks_fts_delete AFTER DELETE ON tasks BEGIN
    INSERT INTO tasks_fts (tasks_fts, rowid, title, notes) VALUES ('delete', old.rowid, old.title, old.notes);
END;

-- Der UPSERT setzt alle Spalten; nur echte Textänderungen indizieren (Verschieben/Abhaken nicht).
CREATE TRIGGER tasks_fts_update AFTER UPDATE OF title, notes ON tasks
WHEN old.title IS NOT new.title OR old.notes IS NOT new.notes BEGIN
    INSERT INTO tasks_fts (tasks_fts, rowid, title, notes) VALUES ('delete', old.rowid, old.title, old.notes);
    INSERT INTO tasks_fts (rowid, title, notes) VALUES (new.rowid, new.title, new.notes);
END;

-- Bestehende Tasks aufnehmen.
INSERT INTO tasks_fts (tasks_fts) VALUES ('rebuild');
