-- 0007: Checkliste an der Task (N8, ADR-034). `checklist` hält die Punkte als Zeilen
-- `id<TAB>0|1<TAB>Text` (Text ist einzeilig, ohne Steuerzeichen); `checklist_text` nur die Texte
-- (eine Zeile je Punkt) für die Suche, die Zähler für die Zeile („2/5“) ohne Parsen.
ALTER TABLE tasks ADD COLUMN checklist TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN checklist_text TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN checklist_done INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN checklist_total INTEGER NOT NULL DEFAULT 0;

-- Volltextsuche (ADR-027) auch über die Punkte: Index mit dritter Spalte neu aufbauen.
DROP TRIGGER tasks_fts_insert;
DROP TRIGGER tasks_fts_delete;
DROP TRIGGER tasks_fts_update;
DROP TABLE tasks_fts;

CREATE VIRTUAL TABLE tasks_fts USING fts5 (
    title,
    notes,
    checklist_text,
    content = 'tasks',
    content_rowid = 'rowid',
    tokenize = 'trigram case_sensitive 0'
);

CREATE TRIGGER tasks_fts_insert AFTER INSERT ON tasks BEGIN
    INSERT INTO tasks_fts (rowid, title, notes, checklist_text)
    VALUES (new.rowid, new.title, new.notes, new.checklist_text);
END;

CREATE TRIGGER tasks_fts_delete AFTER DELETE ON tasks BEGIN
    INSERT INTO tasks_fts (tasks_fts, rowid, title, notes, checklist_text)
    VALUES ('delete', old.rowid, old.title, old.notes, old.checklist_text);
END;

CREATE TRIGGER tasks_fts_update AFTER UPDATE OF title, notes, checklist_text ON tasks
WHEN old.title IS NOT new.title OR old.notes IS NOT new.notes
  OR old.checklist_text IS NOT new.checklist_text BEGIN
    INSERT INTO tasks_fts (tasks_fts, rowid, title, notes, checklist_text)
    VALUES ('delete', old.rowid, old.title, old.notes, old.checklist_text);
    INSERT INTO tasks_fts (rowid, title, notes, checklist_text)
    VALUES (new.rowid, new.title, new.notes, new.checklist_text);
END;

INSERT INTO tasks_fts (tasks_fts) VALUES ('rebuild');
