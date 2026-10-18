import type { CoreClient, TaskDto, TaskSummaryDto } from "@tasks/core-client";
import { toasts } from "../components/toasts.svelte";

/** Wie im Core (`BULK_LIMIT_MAX`): größere Auswahlen gehen in mehreren Aufrufen. */
export const BULK_LIMIT = 2000;

/** Teilt in Pakete zu höchstens `BULK_LIMIT` (eine Auswahl über eine riesige Ansicht). */
function chunks<T>(items: readonly T[]): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += BULK_LIMIT) out.push(items.slice(i, i + BULK_LIMIT));
  return out;
}

/** `call` je Paket nacheinander (mit dem vorigen Paket), Ergebnisse aneinandergehängt. */
async function inChunks<T, R>(items: readonly T[], call: (chunk: T[], previous?: T[]) => Promise<R[]>): Promise<R[]> {
  const done: R[] = [];
  let previous: T[] | undefined;
  for (const chunk of chunks(items)) {
    done.push(...(await call(chunk, previous)));
    previous = chunk;
  }
  return done;
}

function short(title: string): string {
  return title.length > 40 ? `${title.slice(0, 39)}…` : title;
}

/** „„Milch“ gelöscht“ für eine Task, „3 Tasks gelöscht“ für mehrere. */
function subject(tasks: readonly { title: string }[]): string {
  return tasks.length === 1 ? `„${short(tasks[0].title)}“` : `${tasks.length} Tasks`;
}

function failed(what: string, e: unknown) {
  toasts.show({ message: `${what}: ${(e as Error).message}`, tone: "danger" });
}

/**
 * Löscht in **einem** Aufruf je 2 000 Tasks (Core schreibt Tombstones) und bietet „Rückgängig“ an: Das legt alle
 * Tasks mit allen Feldern in einem Aufruf neu an – mit neuen IDs, an ihrer alten Stelle (alter
 * Sort-Key, ADR-021). Liefert die gelöschten Tasks bzw. `null` bei Fehler (Toast ist gezeigt,
 * nichts ist gelöscht).
 */
export async function deleteManyWithUndo(
  client: CoreClient,
  ids: readonly string[],
  onRestored?: (tasks: TaskDto[]) => void,
): Promise<TaskDto[] | null> {
  let deleted: TaskDto[];
  try {
    deleted = await inChunks(ids, (chunk) => client.deleteTasks(chunk));
  } catch (e) {
    failed("Nicht gelöscht", e);
    return null;
  }
  if (deleted.length === 0) return deleted;
  toasts.show({
    message: `${subject(deleted)} gelöscht`,
    actionLabel: "Rückgängig",
    timeoutMs: 6000,
    onAction: () =>
      void inChunks(deleted, (chunk) => client.restoreTasks(chunk)).then(
        (restored) => onRestored?.(restored),
        (e: unknown) => failed("Wiederherstellen fehlgeschlagen", e),
      ),
  });
  return deleted;
}

/**
 * Hakt ab bzw. öffnet wieder – ein Aufruf – und bietet „Rückgängig“ für genau die geänderten Tasks.
 * Liefert sie bzw. `null` bei Fehler (Toast ist gezeigt).
 */
export async function completeManyWithUndo(
  client: CoreClient,
  ids: readonly string[],
  done: boolean,
): Promise<TaskDto[] | null> {
  let changed: TaskDto[];
  try {
    changed = await inChunks(ids, (chunk) => client.completeTasks(chunk, done));
  } catch (e) {
    failed("Nicht gespeichert", e);
    return null;
  }
  if (changed.length === 0) return changed;
  toasts.show({
    message: `${subject(changed)} ${done ? "erledigt" : "wieder geöffnet"}`,
    actionLabel: "Rückgängig",
    onAction: () =>
      void inChunks(
        changed.map((t) => t.id),
        (chunk) => client.completeTasks(chunk, !done),
      ).catch((e: unknown) => failed("Rückgängig fehlgeschlagen", e)),
  });
  return changed;
}

/**
 * Verschiebt die Tasks als Block (ein Aufruf) und bietet „Rückgängig“: Jede Task kommt zurück in
 * ihre Liste an ihren alten Platz. `before` ist der Stand vor dem Verschieben (Liste + Sort-Key).
 * Liefert die geänderten Tasks bzw. `null` bei Fehler (Toast ist gezeigt).
 */
export async function moveManyWithUndo(
  client: CoreClient,
  before: readonly TaskSummaryDto[],
  target: { listId?: string; after?: string; before?: string },
  label: string,
): Promise<TaskDto[] | null> {
  let moved: TaskDto[];
  try {
    // Weitere Pakete direkt hinter das vorige (Eingabe ist in Anzeigereihenfolge): der Block bleibt
    // zusammen und in Reihenfolge.
    moved = await inChunks(before, (chunk, previous) => {
      const last = previous?.at(-1);
      const to = last ? { listId: target.listId ?? last.listId, after: last.id } : target;
      return client.moveTasks(
        chunk.map((t) => t.id),
        to,
      );
    });
  } catch (e) {
    failed("Nicht verschoben", e);
    return null;
  }
  if (moved.length === 0) return moved;
  const changed = new Set(moved.map((t) => t.id));
  const undo = before.filter((t) => changed.has(t.id));
  toasts.show({
    message: `${subject(undo)} ${label}`,
    actionLabel: "Rückgängig",
    timeoutMs: 6000,
    onAction: () => void moveBack(client, undo).catch((e: unknown) => failed("Rückgängig fehlgeschlagen", e)),
  });
  return moved;
}

const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
const byKey = (a: TaskSummaryDto, b: TaskSummaryDto) => cmp(a.sortKey, b.sortKey) || cmp(a.id, b.id);

/**
 * Bringt verschobene Tasks an ihren alten Platz: je Quellliste die übrigen Tasks laden und jede
 * Task vor den ersten übrigen mit größerem (Sort-Key, ID) setzen. Tasks, die vorher zwischen
 * denselben Nachbarn lagen, gehen in **einem** `moveTasks` (Reihenfolge bleibt); eine
 * zusammenhängende Auswahl braucht so einen Aufruf je Liste.
 */
async function moveBack(client: CoreClient, tasks: readonly TaskSummaryDto[]): Promise<void> {
  const moving = new Set(tasks.map((t) => t.id));
  const byList = new Map<string, TaskSummaryDto[]>();
  for (const t of tasks) byList.set(t.listId, [...(byList.get(t.listId) ?? []), t]);
  for (const [listId, group] of byList) {
    const rest = (await allOf(client, listId)).filter((t) => !moving.has(t.id));
    group.sort(byKey);
    let i = 0;
    while (i < group.length) {
      const slot = firstAfter(rest, group[i]);
      let j = i + 1;
      while (j < group.length && firstAfter(rest, group[j]) === slot) j++;
      const ids = group.slice(i, j).map((t) => t.id);
      const target = slot < rest.length ? { listId, before: rest[slot].id } : { listId, after: rest.at(-1)?.id };
      await inChunks(ids, (chunk, previous) =>
        client.moveTasks(chunk, previous ? { listId, after: previous.at(-1) } : target),
      );
      i = j;
    }
  }
}

/** Index der ersten Task in `sorted` mit größerem (Sort-Key, ID) als `t` (binäre Suche). */
function firstAfter(sorted: readonly TaskSummaryDto[], t: TaskSummaryDto): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (byKey(sorted[mid], t) <= 0) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Alle Tasks einer Liste (offen und erledigt), seitenweise. */
async function allOf(client: CoreClient, listId: string): Promise<TaskSummaryDto[]> {
  const items: TaskSummaryDto[] = [];
  let cursor: string | null = null;
  do {
    const page = await client.listTasks({ view: { kind: "list", listId } }, { cursor, limit: 500 });
    items.push(...page.items);
    cursor = page.nextCursor;
  } while (cursor);
  return items.sort(byKey);
}
