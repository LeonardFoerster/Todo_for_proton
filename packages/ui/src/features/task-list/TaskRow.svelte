<script lang="ts">
  import type { TaskSummaryDto } from "@tasks/core-client";
  import Checkbox from "../../components/Checkbox.svelte";
  import { Flag, GripVertical, Hash, Trash } from "../../icons";
  import { dueLabel } from "../../lib/format-date";

  interface Props {
    task: TaskSummaryDto;
    /** Listenname in Sammelansichten (Heute, Geplant, Alle). */
    listName?: string;
    selected?: boolean;
    ontoggle: (task: TaskSummaryDto, done: boolean) => void;
    onopen: (task: TaskSummaryDto) => void;
    ondelete?: (task: TaskSummaryDto) => void;
    /** Ziehen am Griff beginnt (nur in sortierbaren Ansichten gesetzt). */
    ondragstart?: (e: PointerEvent, task: TaskSummaryDto) => void;
    dragging?: boolean;
  }

  let { task, listName, selected = false, ontoggle, onopen, ondelete, ondragstart, dragging = false }: Props = $props();

  const done = $derived(task.status === "done");
  const due = $derived(task.due ? dueLabel(task.due) : null);
  const priorityLabel = { none: "", low: "Niedrige Priorität", medium: "Mittlere Priorität", high: "Hohe Priorität" };
</script>

<div
  class="row"
  class:done
  class:selected
  class:dragging
  role="option"
  aria-selected={selected}
  aria-label={[task.title, done ? "erledigt" : null, due?.long].filter(Boolean).join(", ")}
  tabindex={selected ? 0 : -1}
  data-task-id={task.id}
  onclick={(e) => {
    e.currentTarget.focus({ preventScroll: true });
    onopen(task);
  }}
  onkeydown={(e) => {
    if (e.key === "Enter" && e.target === e.currentTarget) onopen(task);
  }}
>
  {#if ondragstart}
    <span
      class="grip"
      aria-hidden="true"
      onpointerdown={(e) => ondragstart(e, task)}
      onclick={(e) => e.stopPropagation()}
    >
      <GripVertical size={14} strokeWidth={1.75} />
    </span>
  {/if}
  <Checkbox
    checked={done}
    label={done ? `Wieder öffnen: ${task.title}` : `Erledigt: ${task.title}`}
    tone={done || task.priority === "none" ? "default" : task.priority}
    tabindex={-1}
    onchange={(value) => ontoggle(task, value)}
  />
  <span class="title">{task.title}</span>
  <span class="meta">
    {#if task.hasNotes}<span class="notes" title="Hat Notizen" aria-label="Hat Notizen"></span>{/if}
    {#if listName}<span class="list"><Hash size={12} strokeWidth={2} aria-hidden="true" />{listName}</span>{/if}
    {#if due}<span class="due {due.tone}" title={due.long}>{due.text}</span>{/if}
    {#if task.priority !== "none" && !done}
      <span class="prio {task.priority}" title={priorityLabel[task.priority]} aria-label={priorityLabel[task.priority]}>
        <Flag size={14} strokeWidth={2} aria-hidden="true" />
      </span>
    {/if}
  </span>
  {#if ondelete}
    <span class="actions">
      <button
        type="button"
        class="action"
        tabindex="-1"
        aria-label="Löschen"
        data-tooltip="Löschen   Entf"
        onclick={(e) => {
          e.stopPropagation();
          ondelete(task);
        }}
      >
        <Trash size={16} strokeWidth={1.75} aria-hidden="true" />
      </button>
    </span>
  {/if}
</div>

<style>
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--row-height);
    padding: 0 var(--space-4) 0 var(--space-6);
    border-bottom: 1px solid var(--color-border);
    cursor: default;
    user-select: none;
    transition: background-color var(--duration-fast) var(--ease);
  }
  .row:hover {
    background: var(--color-bg-hover);
  }
  .row.selected {
    background: var(--color-bg-selected);
  }
  .row.selected::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--color-accent);
  }
  .grip {
    position: absolute;
    left: 4px;
    top: 50%;
    display: grid;
    place-items: center;
    width: 16px;
    height: 24px;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    opacity: 0;
    transform: translateY(-50%);
    cursor: grab;
    touch-action: none;
  }
  .row:hover .grip {
    opacity: 1;
  }
  .grip:hover {
    background: var(--color-bg-active);
  }
  .row.dragging {
    background: var(--color-bg-selected);
    opacity: 0.6;
  }
  .row:focus-visible {
    outline: 2px solid var(--color-focus-ring);
    outline-offset: -2px;
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    transition: color var(--duration-base) var(--ease);
  }
  .done .title {
    color: var(--color-text-muted);
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--color-text-muted) 70%, transparent);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex: none;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .notes {
    width: 12px;
    height: 2px;
    border-radius: 1px;
    background: currentColor;
    box-shadow:
      0 4px 0 currentColor,
      0 -4px 0 currentColor;
    opacity: 0.7;
  }
  .list {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    max-width: 140px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
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
  .done .due {
    color: var(--color-text-muted);
    font-weight: var(--weight-regular);
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
  .actions {
    position: absolute;
    right: var(--space-2);
    top: 50%;
    display: none;
    gap: 2px;
    padding-left: var(--space-2);
    transform: translateY(-50%);
    background: inherit;
  }
  .row:hover .actions {
    display: flex;
    background: var(--color-bg-hover);
  }
  .row.selected:hover .actions {
    background: var(--color-bg-selected);
  }
  .action {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }
  .action:hover {
    background: var(--color-bg-active);
    color: var(--color-danger);
  }
</style>
