<script lang="ts">
  /** Karte im Kanban-Board: feste Höhe (Virtualisierung), Haken, Titel, Meta – wie `TaskRow`. */
  import type { TaskSummaryDto } from "@tasks/core-client";
  import Checkbox from "../../components/Checkbox.svelte";
    import { dueLabel } from "../../lib/format-date";
  import { priorityInLabel, priorityLabel } from "../task-list/priority";
  import TaskMeta from "../task-list/TaskMeta.svelte";

  interface Props {
    task: TaskSummaryDto;
    /** Cursor (fokussierbar, Akzentrahmen). */
    cursor?: boolean;
    dragging?: boolean;
    ontoggle: (task: TaskSummaryDto, done: boolean) => void;
    onopen: (task: TaskSummaryDto) => void;
    /** Ziehen beginnt (P2-07). */
    onpointerdown?: (e: PointerEvent, task: TaskSummaryDto) => void;
    /** Namen der Tags (ADR-035). */
    tagNames?: readonly string[];
  }

  let { task, cursor = false, dragging = false, ontoggle, onopen, onpointerdown, tagNames = [] }: Props = $props();

  const done = $derived(task.status === "done");
  const due = $derived(task.due ? dueLabel(task.due) : null);
  const priority = $derived(done ? "" : priorityLabel[task.priority]);
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
    aria-label={[task.title, done ? "erledigt" : null, due?.long, task.recurring ? "wiederholt sich" : null, task.hasLinks ? "mit Notiz verknüpft" : null, tagNames.length > 0 ? tagNames.map((t) => `#${t}`).join(" ") : null, done ? null : priorityInLabel(task.priority)].filter(Boolean).join(", ")}
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
      tooltip={priority || undefined}
      onchange={(value) => ontoggle(task, value)}
    />
    <span class="body">
      <span class="title">{task.title}</span>
      {#if due || task.hasNotes || task.hasLinks || task.recurring || task.checklistTotal > 0 || tagNames.length > 0}<TaskMeta {task} {due} size="card" tags={tagNames} />{/if}
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
    position: relative;
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
  /* Cursor wie in der Liste (P5-04): Auswahlfläche + Akzentbalken links, kein Akzentrahmen. */
  .card.cursor {
    border-color: transparent;
    background: var(--color-bg-selected);
  }
  .card.cursor::before {
    content: "";
    position: absolute;
    left: -1px;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 0 2px 2px 0;
    background: var(--color-accent);
  }
  .card.dragging {
    opacity: 0.5;
  }
  .card:focus-visible,
  :global([data-input="keyboard"]) .card:focus {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: -1px;
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
</style>
