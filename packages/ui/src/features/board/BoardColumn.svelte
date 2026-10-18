<script lang="ts" module>
  /** Feste Höhe eines Kartenplatzes (Karte + Abstand) für Virtualisierung und Ziehen. */
  export const CARD_SLOT = 64;
</script>

<script lang="ts">
  /** Eine Spalte des Boards: Kopf (Name, Anzahl, Listenaktionen) und virtualisierte Karten. */
  import type { TaskSummaryDto } from "@tasks/core-client";
  import IconButton from "../../components/IconButton.svelte";
  import VirtualList from "../../components/VirtualList.svelte";
  import { Ellipsis, Hash, Inbox } from "../../icons";
  import { listName } from "../../lib/views";
  import type { BoardColumn } from "./board-state.svelte";
  import BoardCard from "./BoardCard.svelte";

  interface Props {
    column: BoardColumn;
    highlighted?: boolean;
    cursorId?: string | null;
    /** IDs, die gerade gezogen werden (P2-07). */
    dragging?: ReadonlySet<string>;
    ontoggle: (task: TaskSummaryDto, done: boolean) => void;
    onopen: (task: TaskSummaryDto) => void;
    oncardpointerdown?: (e: PointerEvent, task: TaskSummaryDto) => void;
    /** Listenaktionen (nicht für den Eingang). */
    onmenu?: (x: number, y: number) => void;
    /** Einfügelinie beim Ziehen vor Karte Nr. `dropIndex` (P2-07). */
    dropIndex?: number | null;
    /** Griff zum Umsortieren der Spalte (P2-08). */
    onheaderpointerdown?: (e: PointerEvent) => void;
  }

  let {
    column,
    highlighted = false,
    cursorId = null,
    dragging,
    ontoggle,
    onopen,
    oncardpointerdown,
    onmenu,
    dropIndex = null,
    onheaderpointerdown,
  }: Props = $props();

  const name = $derived(listName(column.list));
  const rows = $derived(column.rows);
  let list = $state<VirtualList<TaskSummaryDto>>();

  export function scrollToIndex(index: number) {
    list?.scrollToIndex(index);
  }

  /** Viewport der Karten (Einfügeposition beim Ziehen). */
  export function viewport(): HTMLElement | undefined {
    return list?.element();
  }
</script>

<section class="column" class:highlighted aria-label={name} data-column-id={column.list.id}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header class:grab={!!onheaderpointerdown} onpointerdown={onheaderpointerdown}>
    {#if column.list.isInbox}
      <Inbox size={16} strokeWidth={1.75} aria-hidden="true" />
    {:else}
      <Hash size={16} strokeWidth={1.75} aria-hidden="true" />
    {/if}
    <h2 class="name">{name}</h2>
    <span class="count" aria-label="{rows.items.length} offene Tasks">{rows.items.length}{rows.hasMore ? "+" : ""}</span>
    {#if onmenu}
      <IconButton
        label="Listenaktionen {name}"
        icon={Ellipsis}
        size={28}
        onpointerdown={(e) => e.stopPropagation()}
        onclick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          onmenu(r.left, r.bottom + 4);
        }}
      />
    {/if}
  </header>
  <div class="cards" style:--card-slot="{CARD_SLOT}px">
    {#if rows.loaded && rows.items.length === 0}
      <p class="empty">Keine offenen Tasks</p>
    {/if}
    <VirtualList
      bind:this={list}
      items={rows.items}
      key={(t) => t.id}
      role="listbox"
      label="Tasks in {name}"
      rowHeight={CARD_SLOT}
      onEndReached={rows.hasMore ? () => void rows.loadMore() : undefined}
    >
      {#snippet row(task)}
        <BoardCard
          {task}
          cursor={task.id === cursorId}
          dragging={dragging?.has(task.id) ?? false}
          {ontoggle}
          {onopen}
          onpointerdown={oncardpointerdown}
        />
      {/snippet}
      {#snippet overlay()}
        {#if dropIndex !== null}<div class="drop-line" style:top="{dropIndex * CARD_SLOT - 4}px"></div>{/if}
      {/snippet}
    </VirtualList>
  </div>
</section>

<style>
  .column {
    display: flex;
    flex-direction: column;
    flex: none;
    width: 280px;
    min-height: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg-subtle);
  }
  .column.highlighted {
    border-color: var(--color-accent);
    box-shadow: inset 0 3px 0 var(--color-accent);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 44px;
    padding: 0 var(--space-2) 0 var(--space-3);
    color: var(--color-text-muted);
  }
  header.grab {
    cursor: grab;
  }
  .name {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .count {
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }
  .cards {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .drop-line {
    position: absolute;
    left: var(--space-2);
    right: var(--space-2);
    height: 2px;
    border-radius: 1px;
    background: var(--color-accent);
    pointer-events: none;
  }
  .empty {
    position: absolute;
    inset: var(--space-3) var(--space-3) auto;
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    text-align: center;
  }
</style>
