<script lang="ts">
  import type { TaskSummaryDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import VirtualList from "../../components/VirtualList.svelte";
  import { ArrowDown, ArrowUp, CircleCheck, PanelRight, Trash, Undo } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import type { ListsState } from "../../lib/lists.svelte";
  import { localDay } from "../../lib/local-day";
  import type { Route } from "../../lib/router.svelte";
  import { deleteWithUndo } from "../../lib/task-actions";
  import { listName, routeIcon, SMART_VIEWS } from "../../lib/views";
  import { TaskListState, toSummary } from "./task-list-state.svelte";
  import TaskRow from "./TaskRow.svelte";

  interface Props {
    route: Route;
    lists: ListsState;
    selectedId?: string | null;
    onopen?: (id: string) => void;
    /** Erste Seite ist gerendert (einmal). */
    onready?: () => void;
  }

  let { route, lists, selectedId = $bindable(null), onopen, onready }: Props = $props();

  const ROW = 36;
  const client = useCoreClient();
  // Pro Ansicht eine Instanz (App rendert TaskList per {#key}); Route ist hier fest.
  // svelte-ignore state_referenced_locally
  const view = route;
  const rows = new TaskListState(client, () => ({ route: view, inboxId: lists.inbox?.id, day: localDay() }));

  /** Reihenfolge ist nur in einer einzelnen Liste änderbar (Sammelansichten mischen Listen). */
  const sortable = view.view === "inbox" || view.view === "list";
  const showListNames = view.view !== "list" && view.view !== "inbox";
  const names = $derived(new Map(lists.lists.map((l) => [l.id, listName(l)])));
  const EmptyIcon = routeIcon(view);
  const emptyText =
    view.view === "list" ? "Diese Liste ist leer." : (SMART_VIEWS.find((v) => v.kind === view.view)?.empty ?? "");

  let list = $state<VirtualList<TaskSummaryDto>>();
  let drag = $state<{ id: string; from: number; to: number } | null>(null);
  let menu = $state<{ task: TaskSummaryDto; x: number; y: number } | null>(null);

  onMount(() => {
    const detach = rows.attach();
    void rows
      .load()
      .then(tick)
      .then(() => requestAnimationFrame(() => onready?.()))
      .catch((e: Error) => toasts.show({ message: `Liste nicht geladen: ${e.message}`, tone: "danger", timeoutMs: 0 }));
    return detach;
  });

  function shortTitle(title: string): string {
    return title.length > 40 ? `${title.slice(0, 39)}…` : title;
  }

  async function toggle(task: TaskSummaryDto, done: boolean) {
    const before = rows.setDone(task.id, done);
    try {
      await client.completeTask(task.id, done);
    } catch (e) {
      if (before) rows.replace(before);
      toasts.show({ message: `Nicht gespeichert: ${(e as Error).message}`, tone: "danger" });
      return;
    }
    if (done) {
      toasts.show({
        message: `„${shortTitle(task.title)}“ erledigt`,
        actionLabel: "Rückgängig",
        onAction: () => void toggle({ ...task, status: "done" }, false),
      });
    }
  }

  function open(task: TaskSummaryDto) {
    selectedId = task.id;
    onopen?.(task.id);
  }

  // --- Sortieren -------------------------------------------------------------------------------

  /** Fokus zurück auf die ausgewählte Zeile (Umsortieren verschiebt DOM-Knoten → Fokus geht verloren). */
  export async function focusSelected() {
    await tick();
    const el = selectedId ? document.querySelector<HTMLElement>(`[data-task-id="${CSS.escape(selectedId)}"]`) : null;
    el?.focus({ preventScroll: true });
  }

  /** Verschiebt optimistisch und speichert; bei Fehler neu laden. */
  async function moveTo(id: string, to: number) {
    const hadFocus = list?.element()?.contains(document.activeElement) ?? false;
    const neighbours = rows.reorder(id, to);
    if (!neighbours) return;
    if (hadFocus) void focusSelected();
    try {
      rows.apply(toSummary(await client.moveTask(id, neighbours)));
    } catch (e) {
      toasts.show({ message: `Nicht verschoben: ${(e as Error).message}`, tone: "danger" });
      await rows.load();
    }
  }

  /** Ausgewählte Task eine Position nach oben (-1) oder unten (+1). */
  export function moveSelected(delta: -1 | 1) {
    if (!sortable || !selectedId) return;
    const i = rows.items.findIndex((t) => t.id === selectedId);
    if (i < 0) return;
    const to = delta < 0 ? i - 1 : i + 2;
    if (to < 0 || to > rows.items.length) return;
    void moveTo(selectedId, to);
    list?.scrollToIndex(Math.max(0, i + delta));
  }

  function startDrag(e: PointerEvent, task: TaskSummaryDto) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const viewport = list?.element();
    if (!viewport) return;
    const from = rows.items.findIndex((t) => t.id === task.id);
    drag = { id: task.id, from, to: from };
    let pointerY = e.clientY;
    let frame = 0;

    const update = () => {
      if (!drag) return;
      const rect = viewport.getBoundingClientRect();
      // Am Rand automatisch scrollen.
      const edge = 40;
      if (pointerY < rect.top + edge) viewport.scrollTop -= Math.ceil((rect.top + edge - pointerY) / 4);
      else if (pointerY > rect.bottom - edge) viewport.scrollTop += Math.ceil((pointerY - rect.bottom + edge) / 4);
      const y = pointerY - rect.top + viewport.scrollTop;
      drag = { ...drag, to: Math.max(0, Math.min(rows.items.length, Math.round(y / ROW))) };
      frame = requestAnimationFrame(update);
    };
    const move = (ev: PointerEvent) => (pointerY = ev.clientY);
    const end = (ev: PointerEvent) => {
      cancelAnimationFrame(frame);
      removeEventListener("pointermove", move);
      removeEventListener("pointerup", end);
      removeEventListener("pointercancel", end);
      const done = drag;
      drag = null;
      if (done && ev.type === "pointerup") void moveTo(done.id, done.to);
    };
    addEventListener("pointermove", move);
    addEventListener("pointerup", end);
    addEventListener("pointercancel", end);
    frame = requestAnimationFrame(update);
  }

  const dropLine = $derived(
    drag && drag.to !== drag.from && drag.to !== drag.from + 1 ? drag.to * ROW : null,
  );

  // --- Löschen & Menü --------------------------------------------------------------------------

  export async function deleteTask(task: TaskSummaryDto | undefined = rows.items.find((t) => t.id === selectedId)) {
    if (!task) return;
    const i = rows.items.findIndex((t) => t.id === task.id);
    const neighbours = { afterId: rows.items[i - 1]?.id, beforeId: rows.items[i + 1]?.id };
    // Auswahl auf die nächste Zeile setzen, damit Entf mehrfach hintereinander geht.
    const next = rows.items[i + 1] ?? rows.items[i - 1];
    if (selectedId === task.id) selectedId = next?.id ?? null;
    rows.remove(task.id);
    await deleteWithUndo(client, task.id, neighbours);
  }

  function menuItems(task: TaskSummaryDto): MenuItem[] {
    const i = rows.items.findIndex((t) => t.id === task.id);
    const done = task.status === "done";
    return [
      { label: "Öffnen", icon: PanelRight, shortcut: "Enter", action: () => open(task) },
      {
        label: done ? "Wieder öffnen" : "Erledigt",
        icon: done ? Undo : CircleCheck,
        shortcut: "Leertaste",
        action: () => void toggle(task, !done),
      },
      {
        label: "Nach oben",
        icon: ArrowUp,
        shortcut: "Alt+↑",
        disabled: !sortable || i <= 0,
        action: () => void moveTo(task.id, i - 1),
      },
      {
        label: "Nach unten",
        icon: ArrowDown,
        shortcut: "Alt+↓",
        disabled: !sortable || i >= rows.items.length - 1,
        action: () => void moveTo(task.id, i + 2),
      },
      { label: "Löschen", icon: Trash, shortcut: "Entf", danger: true, action: () => void deleteTask(task) },
    ];
  }

  function contextMenu(e: MouseEvent) {
    const row = (e.target as HTMLElement).closest<HTMLElement>("[data-task-id]");
    const task = row && rows.items.find((t) => t.id === row.dataset.taskId);
    if (!task) return;
    e.preventDefault();
    selectedId = task.id;
    menu = { task, x: e.clientX, y: e.clientY };
  }

  /** Für Tastatur-/Detail-Steuerung von außen. */
  export function items(): readonly TaskSummaryDto[] {
    return rows.items;
  }

  export function scrollToIndex(index: number) {
    list?.scrollToIndex(index);
  }

  export function toggleTask(task: TaskSummaryDto) {
    void toggle(task, task.status !== "done");
  }
</script>

<div class="task-list" class:dragging={drag !== null}>
  {#if rows.loaded && rows.items.length === 0}
    <div class="empty">
      <span class="empty-icon"><EmptyIcon size={28} strokeWidth={1.5} aria-hidden="true" /></span>
      <p>{emptyText}</p>
    </div>
  {:else}
    <VirtualList
      bind:this={list}
      items={rows.items}
      key={(t) => t.id}
      role="listbox"
      label="Tasks"
      rowHeight={ROW}
      onEndReached={rows.hasMore ? () => void rows.loadMore() : undefined}
      oncontextmenu={contextMenu}
    >
      {#snippet row(task)}
        <TaskRow
          {task}
          listName={showListNames ? names.get(task.listId) : undefined}
          selected={task.id === selectedId}
          dragging={drag?.id === task.id}
          ontoggle={toggle}
          onopen={open}
          ondelete={(t) => void deleteTask(t)}
          ondragstart={sortable ? startDrag : undefined}
        />
      {/snippet}
      {#snippet overlay()}
        {#if dropLine !== null}<div class="drop-line" style:top="{dropLine - 1}px"></div>{/if}
      {/snippet}
    </VirtualList>
  {/if}
</div>

{#if menu}
  <Menu label="Aktionen für {menu.task.title}" items={menuItems(menu.task)} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}

<style>
  .task-list {
    height: 100%;
    border-top: 1px solid var(--color-border);
  }
  .task-list.dragging {
    cursor: grabbing;
    user-select: none;
  }
  .drop-line {
    position: absolute;
    left: var(--space-4);
    right: var(--space-4);
    height: 2px;
    border-radius: 1px;
    background: var(--color-accent);
    pointer-events: none;
  }
  .drop-line::before {
    content: "";
    position: absolute;
    left: -4px;
    top: -3px;
    width: 8px;
    height: 8px;
    border: 2px solid var(--color-accent);
    border-radius: var(--radius-full);
    background: var(--color-bg);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    height: 100%;
    padding: var(--space-8);
    color: var(--color-text-muted);
    text-align: center;
  }
  .empty-icon {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    border-radius: var(--radius-full);
    background: var(--color-bg-subtle);
    color: var(--color-text-muted);
  }
  .empty p {
    max-width: 320px;
    margin: 0;
  }
</style>
