<script lang="ts">
  import type { TaskSummaryDto } from "@tasks/core-client";
  import { onMount, tick, untrack } from "svelte";
  import Button from "../../components/Button.svelte";
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import VirtualList from "../../components/VirtualList.svelte";
  import { ArrowDown, ArrowUp, CircleCheck, FolderInput, Hash, Inbox, PanelRight, Plus, Trash, Undo } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import type { ListsState } from "../../lib/lists.svelte";
  import { localDay } from "../../lib/local-day";
  import type { Route } from "../../lib/router.svelte";
  import { completeManyWithUndo, deleteManyWithUndo, moveManyWithUndo } from "../../lib/task-actions";
  import { listName, routeIcon, SMART_VIEWS } from "../../lib/views";
  import { SelectionState } from "./selection.svelte";
  import { TaskListState, toSummary } from "./task-list-state.svelte";
  import TaskRow from "./TaskRow.svelte";

  interface Props {
    route: Route;
    lists: ListsState;
    selectedId?: string | null;
    onopen?: (id: string) => void;
    /** Erste Seite ist gerendert (einmal). */
    onready?: () => void;
    /** Leere Ansicht: „Neue Task“ anbieten (fehlt in „Erledigt“). */
    onnewtask?: () => void;
  }

  let { route, lists, selectedId = $bindable(null), onopen, onready, onnewtask }: Props = $props();

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

  /** Mehrfachauswahl dieser Ansicht (P0); endet mit der Ansicht. */
  export const selection = new SelectionState();

  let list = $state<VirtualList<TaskSummaryDto>>();
  /**
   * Laufendes Ziehen. Zieht man eine ausgewählte Zeile, wandert der ganze Block (`block` = Anzahl,
   * `x`/`y` = Zeiger für das Zähler-Badge).
   */
  let drag = $state<{
    id: string;
    from: number;
    to: number;
    block: number;
    x: number;
    y: number;
    /** Ablegen innerhalb [first, last + 1] ändert nichts (nur bei zusammenhängendem Block). */
    noop: [number, number] | null;
  } | null>(null);
  /** IDs, die gerade gezogen werden (eine Zeile oder der Auswahlblock). */
  let dragIds = $state.raw<ReadonlySet<string>>(new Set());
  let menu = $state<{ task: TaskSummaryDto; x: number; y: number } | null>(null);
  let moveMenu = $state<{ x: number; y: number } | null>(null);

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

  // --- Mehrfachauswahl -------------------------------------------------------------------------

  /** IDs in Anzeigereihenfolge (O(n), nur für Bereiche). */
  const order = () => rows.items.map((t) => t.id);

  /** Auswahl gemessen an der ganzen Ansicht (Tri-State „Alle auswählen“). */
  const allState = $derived<"none" | "some" | "all">(
    selection.count === 0 ? "none" : !rows.hasMore && selection.count >= rows.items.length ? "all" : "some",
  );

  /** Offene bzw. erledigte Tasks in der Auswahl (Aktionsleiste). */
  const flags = $derived(selection.flags((id) => rows.byId(id)));

  // Zeilen, die die Ansicht verlassen (Sync, Löschen, Verschieben), fallen aus der Auswahl – ein
  // Effekt über die ganze Menge, nicht pro Zeile.
  $effect(() => {
    const items = rows.items;
    untrack(() => {
      if (selection.count > 0) selection.prune(new Set(items.map((t) => t.id)));
    });
  });

  /**
   * Klick auf eine Zeile. Ohne Modus öffnet er; Strg/Umschalt starten den Modus (Cursor-Zeile bzw.
   * Bereich ab Cursor, wie im Dateimanager). Im Modus: Klick/Strg schaltet um, Umschalt fügt den
   * Bereich Anker → Zeile hinzu.
   */
  function rowClick(task: TaskSummaryDto, e: MouseEvent) {
    const additive = e.ctrlKey || e.metaKey;
    if (!selection.active) {
      if (!additive && !e.shiftKey) {
        open(task);
        return;
      }
      const cursor = selectedId && rows.byId(selectedId) ? selectedId : null;
      if (e.shiftKey) {
        selection.enter();
        selection.range(task.id, order(), cursor);
      } else {
        selection.enter(cursor ? [cursor] : []);
        if (task.id !== cursor) selection.toggle(task.id);
      }
    } else if (e.shiftKey) {
      selection.range(task.id, order(), selectedId);
    } else {
      selection.toggle(task.id);
    }
    selectedId = task.id;
  }

  export function startSelection() {
    selection.enter();
  }

  export function exitSelection() {
    selection.exit();
  }

  export function selectionAll(): "none" | "some" | "all" {
    return allState;
  }

  export function selectionFlags(): { hasOpen: boolean; hasDone: boolean } {
    return flags;
  }

  /** Alle Tasks der Ansicht (fehlende Seiten werden nachgeladen) bzw. – sind es schon alle – keine. */
  export async function toggleAll() {
    selection.enter();
    if (allState === "all") {
      selection.clear();
      return;
    }
    selection.selectAll(await rows.loadAll());
  }

  /** `X`/Leertaste im Modus: Cursor-Zeile umschalten. */
  export function toggleCursor() {
    if (selectedId && rows.byId(selectedId)) selection.toggle(selectedId);
  }

  /** Umschalt+↑/↓: Cursor bewegen und Auswahl erweitern (startet den Modus). */
  export function extendSelection(delta: -1 | 1) {
    selection.enter();
    const ids = order();
    const next = selection.extend(delta, selectedId && rows.byId(selectedId) ? selectedId : null, ids);
    if (!next) return;
    selectedId = next;
    list?.scrollToIndex(ids.indexOf(next));
    void focusSelected();
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
    const block = selection.active && selection.has(task.id) && selection.count > 1;
    dragIds = block ? new Set(selection.ids) : new Set([task.id]);
    const indices = block ? rows.items.flatMap((t, i) => (dragIds.has(t.id) ? [i] : [])) : [from];
    const [first, last] = [indices[0], indices[indices.length - 1]];
    const noop: [number, number] | null = last - first === indices.length - 1 ? [first, last + 1] : null;
    drag = { id: task.id, from, to: from, block: dragIds.size, x: e.clientX, y: e.clientY, noop };
    let pointer = { x: e.clientX, y: e.clientY };
    let frame = 0;

    const update = () => {
      if (!drag) return;
      const rect = viewport.getBoundingClientRect();
      // Am Rand automatisch scrollen.
      const edge = 40;
      if (pointer.y < rect.top + edge) viewport.scrollTop -= Math.ceil((rect.top + edge - pointer.y) / 4);
      else if (pointer.y > rect.bottom - edge) viewport.scrollTop += Math.ceil((pointer.y - rect.bottom + edge) / 4);
      const y = pointer.y - rect.top + viewport.scrollTop;
      drag = { ...drag, to: Math.max(0, Math.min(rows.items.length, Math.round(y / ROW))), ...pointer };
      frame = requestAnimationFrame(update);
    };
    const move = (ev: PointerEvent) => (pointer = { x: ev.clientX, y: ev.clientY });
    const end = (ev: PointerEvent) => {
      cancelAnimationFrame(frame);
      removeEventListener("pointermove", move);
      removeEventListener("pointerup", end);
      removeEventListener("pointercancel", end);
      const done = drag;
      const ids = dragIds;
      drag = null;
      dragIds = new Set();
      if (!done || ev.type !== "pointerup") return;
      if (done.block > 1) void moveBlockTo(ids, done.to);
      else void moveTo(done.id, done.to);
    };
    addEventListener("pointermove", move);
    addEventListener("pointerup", end);
    addEventListener("pointercancel", end);
    frame = requestAnimationFrame(update);
  }

  /** Einfügelinie nur, wenn das Ablegen etwas ändert (nicht auf dem eigenen, zusammenhängenden Block). */
  const dropLine = $derived(
    drag && !(drag.noop && drag.to >= drag.noop[0] && drag.to <= drag.noop[1]) ? drag.to * ROW : null,
  );

  // --- Auswahlblock umsortieren (P0-06) ---------------------------------------------------------

  /**
   * Block (Auswahl, auch verstreut – wird zusammengezogen) optimistisch an Einfügeposition `to`
   * schieben und mit einem `moveTasks` speichern; Rückgängig im Toast. Bei Fehler neu laden.
   */
  async function moveBlockTo(ids: ReadonlySet<string>, to: number) {
    if (!sortable) return;
    const before = rows.items.filter((t) => ids.has(t.id));
    const hadFocus = list?.element()?.contains(document.activeElement) ?? false;
    const neighbours = rows.reorderMany(ids, to);
    if (!neighbours) return;
    if (hadFocus) void focusSelected();
    const cursor = rows.items.findIndex((t) => t.id === selectedId);
    if (cursor >= 0) list?.scrollToIndex(cursor);
    const moved = await moveManyWithUndo(client, before, { listId: currentList(), ...neighbours }, "verschoben");
    if (!moved) await rows.reload();
  }

  /** Alt+↑/↓ im Modus: Auswahlblock um eine Position (über die erste bzw. unter die letzte Zeile). */
  export function moveSelectionBlock(delta: -1 | 1) {
    if (!sortable || selection.count === 0) return;
    const indices = rows.items.flatMap((t, i) => (selection.has(t.id) ? [i] : []));
    if (indices.length === 0) return;
    const to = delta < 0 ? Math.max(0, indices[0] - 1) : Math.min(rows.items.length, indices[indices.length - 1] + 2);
    void moveBlockTo(new Set(selection.ids), to);
  }

  // --- Löschen & Menü --------------------------------------------------------------------------

  // --- Sammelaktionen (ein Core-Aufruf je Aktion, Rückgängig im Toast) -------------------------

  /** Ausgewählte Zeilen in Anzeigereihenfolge („Alle auswählen“ lädt vorher alle Seiten). */
  const selectedRows = () => rows.items.filter((t) => selection.has(t.id));
  const tasks = (n: number) => (n === 1 ? "1 Task" : `${n} Tasks`);

  /** Cursor von Zeilen weg, die gleich verschwinden: nächste verbliebene, sonst vorige. */
  function moveCursorAway(gone: ReadonlySet<string>) {
    if (!selectedId || !gone.has(selectedId)) return;
    const i = rows.items.findIndex((t) => t.id === selectedId);
    const next =
      rows.items.slice(i + 1).find((t) => !gone.has(t.id)) ??
      rows.items
        .slice(0, i)
        .reverse()
        .find((t) => !gone.has(t.id));
    selectedId = next?.id ?? null;
  }

  /** Cursor-Zeile ins Bild holen und fokussieren (nach dem Entfernen vieler Zeilen). */
  function revealCursor() {
    const i = rows.items.findIndex((t) => t.id === selectedId);
    if (i >= 0) list?.scrollToIndex(i);
    void focusSelected();
  }

  /** Optimistisch ausblenden, löschen; bei Fehler wieder einblenden (FIX-08). */
  async function deleteMany(targets: readonly TaskSummaryDto[]) {
    if (targets.length === 0) return;
    const ids = targets.map((t) => t.id);
    moveCursorAway(new Set(ids));
    rows.hide(ids);
    revealCursor();
    if (await deleteManyWithUndo(client, ids)) selection.clear();
    else await rows.unhide(ids);
  }

  export async function deleteTask(task: TaskSummaryDto | undefined = rows.items.find((t) => t.id === selectedId)) {
    if (task) await deleteMany([task]);
  }

  export async function deleteSelected() {
    await deleteMany(selectedRows());
  }

  /** Alle ausgewählten abhaken (`done`) bzw. wieder öffnen. */
  export async function completeSelected(done: boolean) {
    const ids = selectedRows()
      .filter((t) => t.status !== (done ? "done" : "open"))
      .map((t) => t.id);
    if (ids.length === 0) return;
    const before = rows.setDoneMany(ids, done);
    if (!(await completeManyWithUndo(client, ids, done))) rows.replaceMany(before);
  }

  /** `E` im Modus: offene abhaken, sonst erledigte wieder öffnen. */
  export function completeOrReopenSelected() {
    if (flags.hasOpen) void completeSelected(true);
    else if (flags.hasDone) void completeSelected(false);
  }

  /** Liste, deren Tasks diese Ansicht zeigt (Liste/Eingang); in Sammelansichten keine. */
  const currentList = () => (view.view === "list" ? view.listId : view.view === "inbox" ? lists.inbox?.id : undefined);

  export async function moveSelectedTo(listId: string) {
    const targets = selectedRows().filter((t) => t.listId !== listId);
    if (targets.length === 0) return;
    const ids = targets.map((t) => t.id);
    // Aus einer Listenansicht verschwinden sie; Sammelansichten zeigen nur den neuen Listennamen.
    const leaving = currentList() !== undefined;
    if (leaving) {
      moveCursorAway(new Set(ids));
      rows.removeMany(ids);
      selection.clear();
      revealCursor();
    }
    const name = names.get(listId) ?? "Liste";
    const moved = await moveManyWithUndo(client, targets, { listId }, `nach „${name}“ verschoben`);
    if (!moved && leaving) await rows.reload();
  }

  /** „Verschieben nach…“ an Position (Viewport) öffnen. */
  export function openMoveMenu(x: number, y: number) {
    if (selection.count > 0) moveMenu = { x, y };
  }

  function moveItems(): MenuItem[] {
    const current = currentList();
    const ordered = [...lists.lists].sort((a, b) => Number(b.isInbox) - Number(a.isInbox));
    return ordered.map((l) => ({
      id: l.id,
      label: listName(l),
      icon: l.isInbox ? Inbox : Hash,
      disabled: l.id === current,
      action: () => void moveSelectedTo(l.id),
    }));
  }

  function bulkMenuItems(at: { x: number; y: number }): MenuItem[] {
    const n = tasks(selection.count);
    const items: MenuItem[] = [];
    if (flags.hasOpen) {
      items.push({ label: `${n} erledigen`, icon: CircleCheck, shortcut: "E", action: () => void completeSelected(true) });
    }
    if (flags.hasDone) {
      items.push({
        label: `${n} wieder öffnen`,
        icon: Undo,
        shortcut: flags.hasOpen ? undefined : "E",
        action: () => void completeSelected(false),
      });
    }
    const unsortable = sortable ? undefined : "Nur im Eingang und in Listen sortierbar";
    items.push(
      { label: "Verschieben nach…", icon: FolderInput, shortcut: "M", action: () => openMoveMenu(at.x, at.y) },
      {
        label: "Nach oben",
        icon: ArrowUp,
        shortcut: "Alt+↑",
        disabled: !sortable,
        title: unsortable,
        action: () => moveSelectionBlock(-1),
      },
      {
        label: "Nach unten",
        icon: ArrowDown,
        shortcut: "Alt+↓",
        disabled: !sortable,
        title: unsortable,
        action: () => moveSelectionBlock(1),
      },
      { label: `${n} löschen`, icon: Trash, shortcut: "Entf", danger: true, action: () => void deleteSelected() },
    );
    return items;
  }

  function menuItems(task: TaskSummaryDto, at: { x: number; y: number }): MenuItem[] {
    if (selection.active && selection.has(task.id)) return bulkMenuItems(at);
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
    if (!selection.active || !selection.has(task.id)) selectedId = task.id;
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
      {#if onnewtask && view.view !== "completed"}
        <Button icon={Plus} onclick={onnewtask} title="Neue Task (N)">Neue Task</Button>
      {/if}
    </div>
  {:else}
    <VirtualList
      bind:this={list}
      items={rows.items}
      key={(t) => t.id}
      role="listbox"
      label="Tasks"
      rowHeight={ROW}
      multiselectable={selection.active}
      onEndReached={rows.hasMore ? () => void rows.loadMore() : undefined}
      oncontextmenu={contextMenu}
    >
      {#snippet row(task)}
        <TaskRow
          {task}
          listName={showListNames ? names.get(task.listId) : undefined}
          selected={task.id === selectedId}
          selectable={selection.active}
          checked={selection.active && selection.has(task.id)}
          dragging={drag !== null && dragIds.has(task.id)}
          ontoggle={toggle}
          onopen={rowClick}
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

{#if drag && drag.block > 1}
  <span class="drag-badge" style:left="{drag.x + 12}px" style:top="{drag.y + 12}px" aria-hidden="true">{drag.block}</span>
{/if}
{#if menu}
  <Menu
    label={selection.active && selection.has(menu.task.id) ? `Aktionen für ${tasks(selection.count)}` : `Aktionen für ${menu.task.title}`}
    items={menuItems(menu.task, menu)}
    x={menu.x}
    y={menu.y}
    onclose={() => (menu = null)}
  />
{/if}
{#if moveMenu}
  <Menu label="Verschieben nach" items={moveItems()} x={moveMenu.x} y={moveMenu.y} onclose={() => (moveMenu = null)} />
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
  .drag-badge {
    position: fixed;
    z-index: 70;
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    border-radius: var(--radius-full);
    background: var(--color-accent-fill);
    color: var(--color-on-accent);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    line-height: 20px;
    text-align: center;
    pointer-events: none;
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
