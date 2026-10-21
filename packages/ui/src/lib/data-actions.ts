import type { CoreClient } from "@tasks/core-client";
import { toasts } from "../components/toasts.svelte";
import { localDay } from "./local-day";

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/**
 * Export (ADR-032, nur Desktop): System-Dialog für den Zielordner, dann ein Hinweis mit Ort und Umfang.
 * Liefert `false`, wenn abgebrochen oder fehlgeschlagen (Hinweis ist gezeigt).
 */
export async function exportAll(client: CoreClient): Promise<boolean> {
  if (!client.data) return false;
  try {
    const result = await client.data.exportAll(localDay().date);
    if (!result) return false;
    toasts.show({
      message: `Exportiert: ${plural(result.tasks, "Task", "Tasks")}, ${plural(result.lists, "Liste", "Listen")}, ${plural(result.notes, "Notiz", "Notizen")} nach ${result.path}`,
      timeoutMs: 10_000,
    });
    return true;
  } catch (e) {
    toasts.show({ message: `Export fehlgeschlagen: ${(e as Error).message}`, tone: "danger" });
    return false;
  }
}

/** Aus einem Export wiederherstellen (`tasks.json` wählen); Ergebnis als Hinweis. */
export async function importFile(client: CoreClient): Promise<boolean> {
  if (!client.data) return false;
  try {
    const report = await client.data.importFile();
    if (!report) return false;
    const parts = [`${plural(report.tasksCreated, "Task", "Tasks")} wiederhergestellt`];
    if (report.listsCreated > 0) parts.push(`${plural(report.listsCreated, "Liste", "Listen")} neu`);
    if (report.tasksSkipped > 0) parts.push(`${report.tasksSkipped} schon vorhanden`);
    toasts.show({ message: parts.join(", "), timeoutMs: 8000 });
    return true;
  } catch (e) {
    toasts.show({ message: `Wiederherstellen fehlgeschlagen: ${(e as Error).message}`, tone: "danger" });
    return false;
  }
}
