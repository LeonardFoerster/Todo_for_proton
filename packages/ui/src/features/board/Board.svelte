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
  import { deleteManyWithUndo } from "../../lib/task-actions";
  import { BoardState, type BoardColumn as Column } from "./board-state.svelte";
  import BoardColumn from "./BoardColumn.svelte";

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
    selectedId = task.id;
    onopen?.(task.id);
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
      onmenu={column.list.isInbox || !oncolumnmenu ? undefined : (x, y) => oncolumnmenu(column.list, x, y)}
    />
  {/each}
</div>

<style>
  .board {
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
