<script lang="ts">
  /**
   * Kanban-Board aller Listen (P2-06): Spalten nebeneinander (Eingang zuerst), darin die offenen
   * Tasks als Karten. Cursor wie in der Listenansicht (`selectedId`); Tastatur über die zentrale
   * Registry in `App` (exportierte Funktionen unten).
   */
  import type { TaskListDto, TaskSummaryDto } from "@tasks/core-client";
  import { onMount, tick, untrack } from "svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { useCoreClient } from "../../lib/context";
  import type { ListsState } from "../../lib/lists.svelte";
  import { deleteManyWithUndo, moveManyWithUndo } from "../../lib/task-actions";
  import { listName } from "../../lib/views";
  import { BoardState, type BoardColumn as Column } from "./board-state.svelte";
  import BoardColumn, { CARD_SLOT } from "./BoardColumn.svelte";

  interface Props {
    lists: ListsState;
    /** Spalte der gewählten Liste (hervorgehoben, ins Bild gescrollt); `undefined` bei „Alle“. */
    highlightId?: string;
    selectedId?: string | null;
    onopen?: (id: string) => void;
    /** Erste Darstellung steht (Startzeit, Sync-Start) – wie bei der Liste. */
    onready?: () => void;
    oncolumnmenu?: (list: TaskListDto, x: number, y: number) => void;
  }

  let { lists, highlightId, selectedId = $bindable(null), onopen, onready, oncolumnmenu }: Props = $props();

  const client = useCoreClient();
  const board = new BoardState(client, () => lists.inbox?.id);
  let boardEl = $state<HTMLDivElement>();
  const columnRefs: Record<string, BoardColumn> = $state({});
  let reported = false;

  // Spalten folgen den Listen (anlegen, umbenennen, löschen, umsortieren).
  $effect(() => {
    const all = lists.lists;
    const loaded = lists.loaded;
    untrack(() => {
      const loads = board.setLists(all);
      if (loaded && !reported) {
        reported = true;
        void loads
          .then(tick)
          .then(() => requestAnimationFrame(() => onready?.()))
          .catch((e: Error) => toasts.show({ message: `Board nicht geladen: ${e.message}`, tone: "danger", timeoutMs: 0 }));
      }
    });
  });

  // Gewählte Liste ins Bild holen.
  $effect(() => {
    const id = highlightId;
    const count = board.columns.length;
    if (!id || count === 0) return;
    void tick().then(() =>
      boardEl?.querySelector(`[data-column-id="${CSS.escape(id)}"]`)?.scrollIntoView?.({ inline: "nearest", block: "nearest" }),
    );
  });

  onMount(() => {
    const detach = board.attach();
    return () => {
      detach();
      board.dispose();
    };
  });

  function shortTitle(title: string): string {
    return title.length > 40 ? `${title.slice(0, 39)}…` : title;
  }

  // --- Cursor ---------------------------------------------------------------------------------

  /** Cursor setzen, Spalte/Karte ins Bild holen, fokussieren. */
  async function setCursor(column: Column, index: number) {
    const task = column.rows.items[index];
    if (!task) return;
    selectedId = task.id;
    columnRefs[column.list.id]?.scrollToIndex(index);
    await focusCard(task.id);
  }

  async function focusCard(id: string) {
    await tick();
    const card = boardEl?.querySelector<HTMLElement>(`[data-task-id="${CSS.escape(id)}"]`);
    card?.closest("section")?.scrollIntoView?.({ inline: "nearest", block: "nearest" });
    card?.focus({ preventScroll: true });
  }

  /** Erste Karte: in der hervorgehobenen Spalte, sonst in der ersten nicht leeren. */
  function firstCursor(): [Column, number] | null {
    const preferred = highlightId ? board.column(highlightId) : undefined;
    const column = preferred?.rows.items.length ? preferred : board.columns.find((c) => c.rows.items.length > 0);
    return column ? [column, 0] : null;
  }

  export function hasCards(): boolean {
    return board.columns.some((c) => c.rows.items.length > 0);
  }

  export function cursorTask(): TaskSummaryDto | undefined {
    if (!selectedId) return undefined;
    const column = board.columnOf(selectedId);
    return column?.rows.byId(selectedId);
  }

  /**
   * Cursor bewegen: `dy` innerhalb der Spalte, `dx` zur nächsten nicht leeren Spalte in dieser
   * Richtung (gleiche Höhe, begrenzt aufs Ende).
   */
  export function moveCursor(dx: -1 | 0 | 1, dy: -1 | 0 | 1) {
    const column = selectedId ? board.columnOf(selectedId) : undefined;
    if (!column || !selectedId) {
      const first = firstCursor();
      if (first) void setCursor(...first);
      return;
    }
    const row = column.rows.indexOf(selectedId);
    if (dx === 0) {
      void setCursor(column, Math.max(0, Math.min(column.rows.items.length - 1, row + dy)));
      return;
    }
    const cols = board.columns;
    for (let i = cols.indexOf(column) + dx; i >= 0 && i < cols.length; i += dx) {
      if (cols[i].rows.items.length > 0) {
        void setCursor(cols[i], Math.min(row, cols[i].rows.items.length - 1));
        return;
      }
    }
  }

  /** Cursor von einer Karte weg, die gleich die Spalte verlässt: nächste darunter, sonst darüber. */
  function moveCursorAway(id: string) {
    if (selectedId !== id) return;
    const column = board.columnOf(id);
    if (!column) return;
    const i = column.rows.indexOf(id);
    const next = column.rows.items[i + 1] ?? column.rows.items[i - 1];
    selectedId = next?.id ?? null;
    if (next) void focusCard(next.id);
  }

  // --- Aktionen -------------------------------------------------------------------------------

  async function toggle(task: TaskSummaryDto, done: boolean) {
    const column = board.columnOf(task.id);
    if (done) moveCursorAway(task.id);
    const before = column?.rows.setDone(task.id, done);
    try {
      await client.completeTask(task.id, done);
    } catch (e) {
      if (before) column?.rows.replace(before);
      toasts.show({ message: `Nicht gespeichert: ${(e as Error).message}`, tone: "danger" });
      return;
    }
    if (done) {
      toasts.show({
        message: `„${shortTitle(task.title)}“ erledigt`,
        actionLabel: "Rückgängig",
        onAction: () => void client.completeTask(task.id, false),
      });
    }
  }

  export function toggleCursor() {
    const task = cursorTask();
    if (task) void toggle(task, task.status !== "done");
  }

  export function openCursor() {
    if (selectedId && cursorTask()) onopen?.(selectedId);
  }

  export async function deleteCursor() {
    const task = cursorTask();
    const column = task ? board.columnOf(task.id) : undefined;
    if (!task || !column) return;
    moveCursorAway(task.id);
    column.rows.hide([task.id]);
    if (!(await deleteManyWithUndo(client, [task.id]))) await column.rows.unhide([task.id]);
  }

  function open(task: TaskSummaryDto) {
    // Ein Klick direkt nach dem Ziehen ist kein Öffnen.
    if (justDragged) return;
    selectedId = task.id;
    onopen?.(task.id);
  }

  // --- Verschieben: Ziehen und Alt+Pfeile (P2-07) --------------------------------------------

  /** Live-Region: meldet Verschiebungen für Screenreader. */
  let announcement = $state("");
  /** Laufendes Ziehen: Karte, Zeiger, Zielspalte und Einfügeposition (Index ohne die Karte). */
  let drag = $state<{ task: TaskSummaryDto; from: string; x: number; y: number; over: string | null; to: number } | null>(
    null,
  );
  let justDragged = false;
  const DRAG_THRESHOLD = 4;
  const EDGE = 48;

  /**
   * Karte an Position `to` der Spalte `target` (Index in der Spalte ohne die Karte) setzen:
   * optimistisch, **ein** `moveTask`. Andere Liste → Toast mit Rückgängig (stellt Liste und Platz
   * wieder her); Fehler → beide Spalten neu laden.
   */
  async function placeCard(task: TaskSummaryDto, target: Column, to: number) {
    const source = board.columnOf(task.id);
    if (!source) return;
    if (source === target) {
      const from = source.rows.indexOf(task.id);
      const neighbours = source.rows.reorder(task.id, to > from ? to + 1 : to);
      if (!neighbours) return;
      void focusCard(task.id);
      try {
        await client.moveTask(task.id, neighbours);
      } catch (e) {
        toasts.show({ message: `Nicht verschoben: ${(e as Error).message}`, tone: "danger" });
        await source.rows.reload();
      }
      return;
    }
    source.rows.removeMany([task.id]);
    const neighbours = target.rows.insertAt({ ...task, listId: target.list.id }, to);
    selectedId = task.id;
    columnRefs[target.list.id]?.scrollToIndex(Math.min(to, target.rows.items.length - 1));
    void focusCard(task.id);
    const name = listName(target.list);
    announcement = `„${task.title}“ nach „${name}“ verschoben`;
    const moved = await moveManyWithUndo(client, [task], { listId: target.list.id, ...neighbours }, `nach „${name}“ verschoben`);
    if (!moved) await Promise.all([source.rows.reload(), target.rows.reload()]);
  }

  /** `Alt+←/→`: Cursor-Karte in die Nachbarspalte (gleiche Höhe); `Alt+↑/↓`: in der Spalte. */
  export function moveCursorCard(dx: -1 | 0 | 1, dy: -1 | 0 | 1) {
    const task = cursorTask();
    const column = task ? board.columnOf(task.id) : undefined;
    if (!task || !column) return;
    const row = column.rows.indexOf(task.id);
    if (dx === 0) {
      const to = row + dy;
      if (to < 0 || to >= column.rows.items.length) return;
      void placeCard(task, column, to);
      return;
    }
    const target = board.columns[board.columns.indexOf(column) + dx];
    if (target) void placeCard(task, target, Math.min(row, target.rows.items.length));
  }

  /** Spalte unter dem Zeiger und Einfügeposition darin (ohne die gezogene Karte). */
  function dropTarget(x: number, y: number, task: TaskSummaryDto): { over: string | null; to: number } {
    for (const column of board.columns) {
      const section = boardEl?.querySelector<HTMLElement>(`[data-column-id="${CSS.escape(column.list.id)}"]`);
      const r = section?.getBoundingClientRect();
      if (!r || x < r.left || x > r.right) continue;
      const viewport = columnRefs[column.list.id]?.viewport();
      const v = viewport?.getBoundingClientRect();
      const offset = v ? y - v.top + (viewport?.scrollTop ?? 0) : 0;
      const count = column.rows.items.filter((t) => t.id !== task.id).length;
      return { over: column.list.id, to: Math.max(0, Math.min(count, Math.round(offset / CARD_SLOT))) };
    }
    return { over: null, to: 0 };
  }

  /** Ziehen beginnt erst nach ein paar Pixeln – ein Klick bleibt ein Klick (Detail öffnen). */
  function startCardDrag(e: PointerEvent, task: TaskSummaryDto) {
    if (e.button !== 0 || (e.target as Element).closest('[role="checkbox"]')) return;
    const start = { x: e.clientX, y: e.clientY };
    const from = board.columnOf(task.id)?.list.id;
    if (!from) return;
    let pointer = start;
    let frame = 0;

    const update = () => {
      if (!drag) return;
      const rect = boardEl?.getBoundingClientRect();
      if (rect && boardEl) {
        if (pointer.x < rect.left + EDGE) boardEl.scrollLeft -= Math.ceil((rect.left + EDGE - pointer.x) / 4);
        else if (pointer.x > rect.right - EDGE) boardEl.scrollLeft += Math.ceil((pointer.x - rect.right + EDGE) / 4);
      }
      const target = dropTarget(pointer.x, pointer.y, task);
      const viewport = target.over ? columnRefs[target.over]?.viewport() : undefined;
      const v = viewport?.getBoundingClientRect();
      if (viewport && v) {
        if (pointer.y < v.top + EDGE) viewport.scrollTop -= Math.ceil((v.top + EDGE - pointer.y) / 4);
        else if (pointer.y > v.bottom - EDGE) viewport.scrollTop += Math.ceil((pointer.y - v.bottom + EDGE) / 4);
      }
      drag = { ...drag, ...pointer, ...target };
      frame = requestAnimationFrame(update);
    };
    const move = (ev: PointerEvent) => {
      pointer = { x: ev.clientX, y: ev.clientY };
      if (!drag && Math.hypot(pointer.x - start.x, pointer.y - start.y) >= DRAG_THRESHOLD) {
        drag = { task, from, ...pointer, ...dropTarget(pointer.x, pointer.y, task) };
        frame = requestAnimationFrame(update);
      }
    };
    const stop = () => {
      cancelAnimationFrame(frame);
      removeEventListener("pointermove", move);
      removeEventListener("pointerup", end);
      removeEventListener("pointercancel", end);
      removeEventListener("keydown", escape, true);
    };
    const end = (ev: PointerEvent) => {
      stop();
      const done = drag;
      drag = null;
      if (!done) return;
      justDragged = true;
      setTimeout(() => (justDragged = false));
      if (ev.type !== "pointerup") return;
      const final = dropTarget(ev.clientX, ev.clientY, task);
      const target = final.over ? board.column(final.over) : undefined;
      if (target) void placeCard(task, target, final.to);
    };
    const escape = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape" || !drag) return;
      ev.preventDefault();
      ev.stopPropagation();
      stop();
      drag = null;
    };
    addEventListener("pointermove", move);
    addEventListener("pointerup", end);
    addEventListener("pointercancel", end);
    addEventListener("keydown", escape, true);
  }

  // --- Spalten umsortieren (P2-08) -------------------------------------------------------------

  /** Ziehen einer Spalte: Liste, Startindex unter den Benutzerlisten, Einfügeposition, Linie (px). */
  let columnDrag = $state<{ id: string; from: number; to: number; line: number } | null>(null);

  const userColumns = () => board.columns.filter((c) => !c.list.isInbox);

  /** Einfügeposition unter den Benutzerlisten aus der Zeiger-X und die Linie im Board (px). */
  function columnDropTarget(x: number): { to: number; line: number } {
    const sections = userColumns().map((c) =>
      boardEl?.querySelector<HTMLElement>(`[data-column-id="${CSS.escape(c.list.id)}"]`)?.getBoundingClientRect(),
    );
    const origin = (boardEl?.getBoundingClientRect().left ?? 0) - (boardEl?.scrollLeft ?? 0);
    for (const [i, r] of sections.entries()) {
      if (r && x < r.left + r.width / 2) return { to: i, line: r.left - origin - 7 };
    }
    const last = sections.at(-1);
    return { to: sections.length, line: last ? last.right - origin + 5 : 0 };
  }

  function startColumnDrag(e: PointerEvent, column: Column) {
    if (e.button !== 0) return;
    const start = e.clientX;
    const from = userColumns().indexOf(column);
    const move = (ev: PointerEvent) => {
      if (!columnDrag && Math.abs(ev.clientX - start) < DRAG_THRESHOLD) return;
      columnDrag = { id: column.list.id, from, ...columnDropTarget(ev.clientX) };
    };
    const end = (ev: PointerEvent) => {
      removeEventListener("pointermove", move);
      removeEventListener("pointerup", end);
      removeEventListener("pointercancel", end);
      const done = columnDrag;
      columnDrag = null;
      if (done && ev.type === "pointerup" && done.to !== done.from && done.to !== done.from + 1) {
        void moveColumn(done.id, done.to);
      }
    };
    addEventListener("pointermove", move);
    addEventListener("pointerup", end);
    addEventListener("pointercancel", end);
  }

  /** Benutzerliste an Einfügeposition `to` – Board und Seitenleiste folgen derselben Reihenfolge. */
  async function moveColumn(id: string, to: number) {
    try {
      await lists.move(id, to);
    } catch (e) {
      toasts.show({ message: `Liste nicht verschoben: ${(e as Error).message}`, tone: "danger" });
    }
  }

  /** Fokussierter Spaltenkopf (für `Alt+←/→`). */
  export function focusedColumn(): string | undefined {
    return (document.activeElement as HTMLElement | null)?.dataset?.columnHeader;
  }

  /** `Alt+←/→` auf einem Spaltenkopf: Spalte um eine Position; Fokus bleibt auf dem Kopf. */
  export async function moveFocusedColumn(dx: -1 | 1) {
    const id = focusedColumn();
    const i = userColumns().findIndex((c) => c.list.id === id);
    if (!id || i < 0) return;
    const to = dx < 0 ? i - 1 : i + 2;
    if (to < 0 || to > userColumns().length) return;
    await moveColumn(id, to);
    await tick();
    boardEl?.querySelector<HTMLElement>(`[data-column-header="${CSS.escape(id)}"]`)?.focus();
  }

  /** Einfügelinie der Spalte, außer das Ablegen ließe die Karte, wo sie ist. */
  function dropIndexFor(column: Column): number | null {
    if (!drag || drag.over !== column.list.id) return null;
    if (drag.from === column.list.id && drag.to === column.rows.indexOf(drag.task.id)) return null;
    // Linie zwischen den sichtbaren Karten: Index inklusive der gezogenen Karte.
    const from = column.rows.indexOf(drag.task.id);
    return from >= 0 && drag.to >= from ? drag.to + 1 : drag.to;
  }
</script>

<div class="board" role="region" aria-label="Kanban-Board" bind:this={boardEl}>
  {#each board.columns as column (column.list.id)}
    <BoardColumn
      bind:this={columnRefs[column.list.id]}
      {column}
      highlighted={column.list.id === highlightId}
      cursorId={selectedId}
      ontoggle={(task, done) => void toggle(task, done)}
      onopen={open}
      oncardpointerdown={startCardDrag}
      dropIndex={dropIndexFor(column)}
      dragging={drag ? new Set([drag.task.id]) : undefined}
      onheaderpointerdown={column.list.isInbox ? undefined : (e) => startColumnDrag(e, column)}
      onmenu={column.list.isInbox || !oncolumnmenu ? undefined : (x, y) => oncolumnmenu(column.list, x, y)}
    />
  {/each}
  {#if columnDrag && columnDrag.to !== columnDrag.from && columnDrag.to !== columnDrag.from + 1}
    <div class="column-drop" style:left="{columnDrag.line}px" aria-hidden="true"></div>
  {/if}
</div>
{#if drag}
  <div class="ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 8}px" aria-hidden="true">{drag.task.title}</div>
{/if}
<div class="sr-only" role="status" aria-live="polite">{announcement}</div>

<style>
  .ghost {
    position: fixed;
    z-index: 70;
    max-width: 240px;
    padding: var(--space-2) var(--space-3);
    overflow: hidden;
    border: 1px solid var(--color-accent);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    box-shadow: var(--shadow-popover);
    white-space: nowrap;
    text-overflow: ellipsis;
    pointer-events: none;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .column-drop {
    position: absolute;
    top: var(--space-3);
    bottom: var(--space-4);
    width: 2px;
    border-radius: 1px;
    background: var(--color-accent);
    pointer-events: none;
  }
  .board {
    position: relative;
    display: flex;
    gap: var(--space-3);
    height: 100%;
    padding: var(--space-3) var(--space-4) var(--space-4);
    overflow-x: auto;
    overflow-y: hidden;
    border-top: 1px solid var(--color-border);
    overscroll-behavior-x: contain;
  }
</style>
