<script lang="ts">
  /** Karte im Kanban-Board: feste Höhe (Virtualisierung), Haken, Titel, Meta – wie `TaskRow`. */
  import type { TaskSummaryDto } from "@tasks/core-client";
  import Checkbox from "../../components/Checkbox.svelte";
  import { Flag } from "../../icons";
  import { dueLabel } from "../../lib/format-date";

  interface Props {
    task: TaskSummaryDto;
    /** Cursor (fokussierbar, Akzentrahmen). */
    cursor?: boolean;
    dragging?: boolean;
    ontoggle: (task: TaskSummaryDto, done: boolean) => void;
    onopen: (task: TaskSummaryDto) => void;
    /** Ziehen beginnt (P2-07). */
    onpointerdown?: (e: PointerEvent, task: TaskSummaryDto) => void;
  }

  let { task, cursor = false, dragging = false, ontoggle, onopen, onpointerdown }: Props = $props();

  const done = $derived(task.status === "done");
  const due = $derived(task.due ? dueLabel(task.due) : null);
  const priorityLabel = { none: "", low: "Niedrige Priorität", medium: "Mittlere Priorität", high: "Hohe Priorität" };
</script>

<!-- Tasten behandelt die zentrale Registry (lib/shortcuts.ts). -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="slot">
  <div
    class="card"
    class:done
    class:cursor
    class:dragging
    role="option"
    aria-label={[task.title, done ? "erledigt" : null, due?.long].filter(Boolean).join(", ")}
    aria-selected={cursor}
    tabindex={cursor ? 0 : -1}
    data-task-id={task.id}
    onclick={(e) => {
      e.currentTarget.focus({ preventScroll: true });
      onopen(task);
    }}
    onpointerdown={onpointerdown ? (e) => onpointerdown(e, task) : undefined}
  >
    <Checkbox
      checked={done}
      label={done ? `Wieder öffnen: ${task.title}` : `Erledigt: ${task.title}`}
      tone={done || task.priority === "none" ? "default" : task.priority}
      tabindex={-1}
      onchange={(value) => ontoggle(task, value)}
    />
    <span class="body">
      <span class="title">{task.title}</span>
      {#if due || task.hasNotes || (task.priority !== "none" && !done)}
        <span class="meta">
          {#if due}<span class="due {due.tone}" title={due.long}>{due.text}</span>{/if}
          {#if task.hasNotes}<span class="notes" title="Hat Notizen" aria-label="Hat Notizen"></span>{/if}
          {#if task.priority !== "none" && !done}
            <span class="prio {task.priority}" title={priorityLabel[task.priority]} aria-label={priorityLabel[task.priority]}>
              <Flag size={12} strokeWidth={2} aria-hidden="true" />
            </span>
          {/if}
        </span>
      {/if}
    </span>
  </div>
</div>

<style>
  /* Feste Zeilenhöhe für die Virtualisierung: Karte + Abstand. */
  .slot {
    height: var(--card-slot, 64px);
    padding: 0 var(--space-2) var(--space-2);
  }
  .card {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    height: 100%;
    padding: var(--space-2) var(--space-3) var(--space-2) var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    cursor: default;
    user-select: none;
    transition:
      border-color var(--duration-fast) var(--ease),
      background-color var(--duration-fast) var(--ease);
  }
  .card:hover {
    border-color: var(--color-border-strong);
  }
  .card.cursor {
    border-color: var(--color-accent);
    background: var(--color-bg-selected);
  }
  .card.dragging {
    opacity: 0.5;
  }
  .card:focus-visible,
  :global([data-input="keyboard"]) .card:focus {
    outline: 2px solid var(--color-focus-ring);
    outline-offset: 1px;
  }
  :global([data-input="pointer"]) .card:focus:not(:focus-visible) {
    outline: none;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
    padding-top: 2px;
  }
  .title {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .done .title {
    color: var(--color-text-muted);
    text-decoration: line-through;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }
  .due {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .due.overdue {
    color: var(--color-danger);
    font-weight: var(--weight-medium);
  }
  .due.today {
    color: var(--color-accent-text);
    font-weight: var(--weight-medium);
  }
  .notes {
    width: 10px;
    height: 2px;
    border-radius: 1px;
    background: currentColor;
    box-shadow:
      0 3px 0 currentColor,
      0 -3px 0 currentColor;
    opacity: 0.7;
  }
  .prio {
    display: grid;
    place-items: center;
  }
  .prio.high {
    color: var(--color-danger);
  }
  .prio.medium {
    color: var(--color-warning);
  }
  .prio.low {
    color: var(--color-accent);
  }
</style>
