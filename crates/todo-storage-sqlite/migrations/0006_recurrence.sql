-- Wiederkehrende Tasks (N4, ADR-028): Regel als iCalendar-ähnlicher Text (`FREQ=WEEKLY;BYDAY=MO`),
-- `NULL` = keine Wiederholung. Die Sync-Ops tragen sie binär (Feld 17), hier zählt Lesbarkeit.
ALTER TABLE tasks ADD COLUMN recurrence TEXT;
