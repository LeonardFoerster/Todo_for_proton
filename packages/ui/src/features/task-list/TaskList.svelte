<script lang="ts">
  import type { TaskSummaryDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import { toasts } from "../../components/toasts.svelte";
  import VirtualList from "../../components/VirtualList.svelte";
  import { useCoreClient } from "../../lib/context";
  import type { ListsState } from "../../lib/lists.svelte";
  import { localDay } from "../../lib/local-day";
  import type { Route } from "../../lib/router.svelte";
  import { listName, routeIcon, SMART_VIEWS } from "../../lib/views";
  import { TaskListState } from "./task-list-state.svelte";
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

  const client = useCoreClient();
  // Pro Ansicht eine Instanz (App rendert TaskList per {#key}); Route ist hier fest.
  // svelte-ignore state_referenced_locally
  const view = route;
  const state = new TaskListState(client, () => ({ route: view, inboxId: lists.inbox?.id, day: localDay() }));

  const showListNames = view.view !== "list" && view.view !== "inbox";
  const names = $derived(new Map(lists.lists.map((l) => [l.id, listName(l)])));
  const EmptyIcon = routeIcon(view);
  const emptyText =
    view.view === "list" ? "Diese Liste ist leer." : (SMART_VIEWS.find((v) => v.kind === view.view)?.empty ?? "");

  onMount(() => {
    const detach = state.attach();
    void state
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
    const before = state.setDone(task.id, done);
    try {
      await client.completeTask(task.id, done);
    } catch (e) {
      if (before) state.replace(before);
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

  /** Für Tastatur-/Detail-Steuerung von außen. */
  export function items(): readonly TaskSummaryDto[] {
    return state.items;
  }
</script>

<div class="task-list">
  {#if state.loaded && state.items.length === 0}
    <div class="empty">
      <span class="empty-icon"><EmptyIcon size={28} strokeWidth={1.5} aria-hidden="true" /></span>
      <p>{emptyText}</p>
    </div>
  {:else}
    <VirtualList
      items={state.items}
      key={(t) => t.id}
      role="listbox"
      label="Tasks"
      onEndReached={state.hasMore ? () => void state.loadMore() : undefined}
    >
      {#snippet row(task)}
        <TaskRow
          {task}
          listName={showListNames ? names.get(task.listId) : undefined}
          selected={task.id === selectedId}
          ontoggle={toggle}
          onopen={open}
        />
      {/snippet}
    </VirtualList>
  {/if}
</div>

<style>
  .task-list {
    height: 100%;
    border-top: 1px solid var(--color-border);
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
