import type { CoreClient, TaskDto } from "@tasks/core-client";
import { toasts } from "../components/toasts.svelte";

function short(title: string): string {
  return title.length > 40 ? `${title.slice(0, 39)}…` : title;
}

/**
 * Löscht sofort (Core schreibt einen Tombstone) und bietet „Rückgängig“ an. Rückgängig legt die
 * Task mit allen Feldern neu an – an derselben Stelle (vor `beforeId`, sonst hinter `afterId`).
 * Eine gelöschte ID lässt sich im Sync-Modell nicht wiederbeleben.
 */
export async function deleteWithUndo(
  client: CoreClient,
  id: string,
  neighbours: { afterId?: string; beforeId?: string } = {},
): Promise<void> {
  let task: TaskDto;
  try {
    task = await client.getTask(id);
    await client.deleteTask(id);
  } catch (e) {
    toasts.show({ message: `Nicht gelöscht: ${(e as Error).message}`, tone: "danger" });
    return;
  }
  toasts.show({
    message: `„${short(task.title)}“ gelöscht`,
    actionLabel: "Rückgängig",
    timeoutMs: 6000,
    onAction: () => void restore(client, task, neighbours),
  });
}

async function restore(client: CoreClient, task: TaskDto, n: { afterId?: string; beforeId?: string }) {
  try {
    const copy = await client.createTask({ title: task.title, listId: task.listId });
    await client.updateTask(copy.id, { notes: task.notes, priority: task.priority, due: task.due });
    if (task.status === "done") await client.completeTask(copy.id, true);
    // Nachbarn können inzwischen weg sein – dann den anderen versuchen, sonst bleibt sie am Ende.
    const targets = [n.beforeId ? { before: n.beforeId } : null, n.afterId ? { after: n.afterId } : null];
    for (const target of targets) {
      if (!target) continue;
      try {
        await client.moveTask(copy.id, target);
        break;
      } catch {
        // nächsten Nachbarn versuchen
      }
    }
  } catch (e) {
    toasts.show({ message: `Wiederherstellen fehlgeschlagen: ${(e as Error).message}`, tone: "danger" });
  }
}
