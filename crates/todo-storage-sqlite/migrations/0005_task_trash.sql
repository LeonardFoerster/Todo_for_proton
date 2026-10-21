-- Papierkorb für Tasks (ADR-031): LWW-Feld `Trashed` wie bei Notizen, `NULL` = nicht gelöscht.
-- Alle Ansichten außer „Papierkorb“ filtern `trashed_at IS NULL`; der Teilindex trägt die Ansicht
-- selbst (zuletzt gelöschte zuerst) und das Aufräumen nach 30 Tagen.
ALTER TABLE tasks ADD COLUMN trashed_at INTEGER;

CREATE INDEX tasks_by_trash ON tasks (trashed_at DESC, id) WHERE trashed_at IS NOT NULL;
