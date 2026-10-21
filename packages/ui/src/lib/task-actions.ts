import type { CompleteResultDto, CoreClient, DueDto, OccurrenceDto, TaskDto, TaskSummaryDto } from "@tasks/core-client";
import { toasts } from "../components/toasts.svelte";
import { localDay, parseIsoDate } from "./local-day";

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
 * „Löschen“ in der UI (ADR-031): legt je 2 000 Tasks in einem Aufruf in den Papierkorb und bietet
 * „Rückgängig“ an – das holt genau diese Tasks mit **gleichen IDs** an ihren alten Platz zurück.
 * Liefert die geänderten Tasks bzw. `null` bei Fehler (Toast ist gezeigt, nichts ist geändert).
 */
export async function trashManyWithUndo(client: CoreClient, ids: readonly string[]): Promise<TaskDto[] | null> {
  let trashed: TaskDto[];
  try {
    trashed = await inChunks(ids, (chunk) => client.trashTasks(chunk));
  } catch (e) {
    failed("Nicht gelöscht", e);
    return null;
  }
  if (trashed.length === 0) return trashed;
  toasts.show({
    // Eine Serie geht als Ganzes (der Verlauf bleibt).
    message: `${trashed.length === 1 && trashed[0].recurrence ? `Serie „${short(trashed[0].title)}“` : subject(trashed)} in den Papierkorb gelegt`,
    actionLabel: "Rückgängig",
    timeoutMs: 6000,
    onAction: () =>
      void untrashMany(client, trashed.map((t) => t.id)),
  });
  return trashed;
}

/** Aus dem Papierkorb zurückholen (Rückgängig, „Wiederherstellen“); Fehler als Toast. */
export async function untrashMany(client: CoreClient, ids: readonly string[]): Promise<TaskDto[] | null> {
  try {
    return await inChunks(ids, (chunk) => client.untrashTasks(chunk));
  } catch (e) {
    failed("Wiederherstellen fehlgeschlagen", e);
    return null;
  }
}

/** „Wiederherstellen“ im Papierkorb mit Bestätigung im Toast. */
export async function untrashWithToast(client: CoreClient, ids: readonly string[]): Promise<TaskDto[] | null> {
  const restored = await untrashMany(client, ids);
  if (restored && restored.length > 0) toasts.show({ message: `${subject(restored)} wiederhergestellt` });
  return restored;
}

/** Endgültig löschen (nur aus dem Papierkorb, nach Bestätigung); Fehler als Toast. */
export async function purgeMany(client: CoreClient, tasks: readonly { id: string; title: string }[]): Promise<number | null> {
  let n = 0;
  try {
    for (const chunk of chunks(tasks)) n += await client.purgeTasks(chunk.map((t) => t.id));
  } catch (e) {
    failed("Nicht gelöscht", e);
    return null;
  }
  if (n > 0) toasts.show({ message: `${n === 1 && tasks.length === 1 ? subject(tasks) : `${n} ${n === 1 ? "Task" : "Tasks"}`} endgültig gelöscht` });
  return n;
}

const NEXT = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short" });

/** „Fr, 31. Okt.“ für den Toast nach dem Abhaken einer Serie. */
export function nextTimeLabel(due: DueDto): string {
  const day = due.kind === "date" ? parseIsoDate(due.date) : new Date(due.at);
  return NEXT.format(day).replace(/^(\p{L}+)\./u, "$1");
}

/**
 * Hakt ab bzw. öffnet wieder – ein Aufruf je 2 000 – und bietet „Rückgängig“ für genau die geänderten
 * Tasks. Serien (ADR-028) rücken vor und bekommen einen Verlaufseintrag; „Rückgängig“ nimmt beides
 * zurück (`undoOccurrence`). Ein Toast für alles. Liefert die geänderten Tasks (ohne Verlaufseinträge)
 * bzw. `null` bei Fehler (Toast ist gezeigt).
 */
export async function completeManyWithUndo(
  client: CoreClient,
  ids: readonly string[],
  done: boolean,
): Promise<TaskDto[] | null> {
  const result: CompleteResultDto = { tasks: [], occurrences: [] };
  try {
    const today = localDay();
    for (const chunk of chunks(ids)) {
      const r = await client.completeTasks(chunk, done, today);
      result.tasks.push(...r.tasks);
      result.occurrences.push(...r.occurrences);
    }
  } catch (e) {
    failed("Nicht gespeichert", e);
    return null;
  }
  const entries = new Set(result.occurrences.map((o) => o.occurrenceId));
  const changed = result.tasks.filter((t) => !entries.has(t.id));
  if (changed.length === 0) return changed;
  const series = new Set(result.occurrences.map((o) => o.seriesId));
  const plain = changed.filter((t) => !series.has(t.id)).map((t) => t.id);
  toasts.show({
    message: completedMessage(changed, result.occurrences, done),
    actionLabel: "Rückgängig",
    onAction: () => void undoComplete(client, plain, result.occurrences, done),
  });
  return changed;
}

function completedMessage(changed: readonly TaskDto[], occurrences: readonly OccurrenceDto[], done: boolean): string {
  if (!done) return `${subject(changed)} wieder geöffnet`;
  const [only] = changed;
  if (changed.length === 1 && occurrences.length === 1) {
    if (!occurrences[0].occurrenceId) return `„${short(only.title)}“ erledigt – Serie beendet`;
    if (only.due) return `„${short(only.title)}“ erledigt – nächstes Mal ${nextTimeLabel(only.due)}`;
  }
  return `${subject(changed)} erledigt`;
}

async function undoComplete(client: CoreClient, plain: string[], occurrences: readonly OccurrenceDto[], done: boolean) {
  try {
    for (const chunk of chunks(plain)) await client.completeTasks(chunk, !done);
    for (const o of occurrences) await client.undoOccurrence(o);
  } catch (e) {
    failed("Rückgängig fehlgeschlagen", e);
  }
}

/** „Dieses Mal überspringen“ (ADR-028) mit Toast; Fehler als Toast. */
export async function skipWithToast(client: CoreClient, task: { id: string; title: string }): Promise<TaskDto | null> {
  try {
    const next = await client.skipOccurrence(task.id, localDay());
    toasts.show({
      message:
        next.status === "done"
          ? `„${short(task.title)}“ übersprungen – Serie beendet`
          : `„${short(task.title)}“ übersprungen – nächstes Mal ${next.due ? nextTimeLabel(next.due) : "–"}`,
    });
    return next;
  } catch (e) {
    failed("Nicht übersprungen", e);
    return null;
  }
}

/**
 * Erledigte Tasks, die vor `cutoff` abgehakt wurden (für „Erledigte aufräumen“, P4-06). Lädt die
 * Ansicht „Erledigt“ seitenweise – nur bei dieser Aktion, nie beim Tippen.
 */
export async function completedBefore(client: CoreClient, cutoff: Date): Promise<TaskSummaryDto[]> {
  const limit = 500;
  const before = cutoff.toISOString();
  const out: TaskSummaryDto[] = [];
  let cursor: string | null = null;
  do {
    const page = await client.listTasks({ view: { kind: "completed" } }, { cursor, limit });
    for (const t of page.items) if (t.completedAt && t.completedAt < before) out.push(t);
    cursor = page.nextCursor;
  } while (cursor);
  return out;
}

/**
 * Setzt je Task eine neue Fälligkeit (`next` aus dem bisherigen Stand, z. B. Uhrzeit behalten) –
 * ein Aufruf je 2 000 – und bietet „Rückgängig“: schreibt die alten Werte zurück. `label` ergänzt
 * den Toast („auf heute verschoben“). Liefert die geänderten Tasks bzw. `null` bei Fehler.
 */
export async function setDuesWithUndo(
  client: CoreClient,
  before: readonly Pick<TaskSummaryDto, "id" | "title" | "due">[],
  next: (task: Pick<TaskSummaryDto, "due">) => DueDto | null,
  label: string,
): Promise<TaskDto[] | null> {
  let changed: TaskDto[];
  try {
    changed = await inChunks(
      before.map((t) => ({ id: t.id, due: next(t) })),
      (chunk) => client.setDues(chunk),
    );
  } catch (e) {
    failed("Nicht gespeichert", e);
    return null;
  }
  if (changed.length === 0) return changed;
  const old = new Map(before.map((t) => [t.id, t.due]));
  toasts.show({
    message: `${subject(changed)} ${label}`,
    actionLabel: "Rückgängig",
    onAction: () =>
      void inChunks(
        changed.map((t) => ({ id: t.id, due: old.get(t.id) ?? null })),
        (chunk) => client.setDues(chunk),
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
