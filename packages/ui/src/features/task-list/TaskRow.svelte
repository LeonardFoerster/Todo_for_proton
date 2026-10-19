<script lang="ts">
  import type { TaskSummaryDto } from "@tasks/core-client";
  import Checkbox from "../../components/Checkbox.svelte";
  import SelectBox from "../../components/SelectBox.svelte";
  import { Flag, GripVertical, Hash, Trash } from "../../icons";
  import { dueLabel, dueToInputTime } from "../../lib/format-date";
  import type { DueMode } from "../../lib/groups";

  interface Props {
    task: TaskSummaryDto;
    /** Listenname in Sammelansichten (Heute, Geplant, Alle). */
    listName?: string;
    /** Cursor: fokussierte Zeile (Tastatur, Detail). */
    selected?: boolean;
    /** Auswahlmodus an: Auswahlkästchen zeigen, `aria-selected` = `checked`. */
    selectable?: boolean;
    /** In der Mehrfachauswahl. */
    checked?: boolean;
    ontoggle: (task: TaskSummaryDto, done: boolean) => void;
    /** Klick auf die Zeile (Modifikatortasten entscheiden in der Liste). */
    onopen: (task: TaskSummaryDto, e: MouseEvent) => void;
    ondelete?: (task: TaskSummaryDto) => void;
    /** Ziehen am Griff beginnt (nur in sortierbaren Ansichten gesetzt). */
    ondragstart?: (e: PointerEvent, task: TaskSummaryDto) => void;
    dragging?: boolean;
    /** `time`: Der Tag steht in der Abschnittsüberschrift – nur eine Uhrzeit zeigen (P4). */
    dueMode?: DueMode;
  }

  let {
    task,
    listName,
    selected = false,
    selectable = false,
    checked = false,
    ontoggle,
    onopen,
    ondelete,
    ondragstart,
    dragging = false,
    dueMode = "full",
  }: Props = $props();

  const done = $derived(task.status === "done");
  const due = $derived(task.due ? dueLabel(task.due) : null);
  /** Angezeigter Text: ganzes Datum, oder nur die Uhrzeit (ganztägig dann nichts). */
  const dueText = $derived(due && (dueMode === "full" ? due.text : dueToInputTime(task.due)));
  const priorityLabel = { none: "", low: "Niedrige Priorität", medium: "Mittlere Priorität", high: "Hohe Priorität" };
</script>

<!-- Tasten (Enter, Leertaste, Entf, Pfeile) behandelt die zentrale Registry (lib/shortcuts.ts). -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="row"
  class:done
  class:selected
  class:selectable
  class:checked
  class:dragging
  role="option"
  aria-selected={selectable ? checked : selected}
  aria-label={[task.title, done ? "erledigt" : null, due?.long].filter(Boolean).join(", ")}
  tabindex={selected ? 0 : -1}
  data-task-id={task.id}
  onclick={(e) => {
    e.currentTarget.focus({ preventScroll: true });
    onopen(task, e);
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
  {#if selectable}<SelectBox {checked} />{/if}
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
    {#if due && dueText}<span class="due {due.tone}" title={due.long}>{dueText}</span>{/if}
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
  .row.selected,
  .row.selectable.checked {
    background: var(--color-bg-selected);
  }
  /* Im Auswahlmodus färbt nur die Auswahl; der Cursor behält den Balken links. */
  .row.selectable.selected:not(.checked) {
    background: transparent;
  }
  .row.selectable.selected:not(.checked):hover {
    background: var(--color-bg-hover);
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
  .row:focus-visible,
  :global([data-input="keyboard"]) .row:focus {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: -2px;
  }
  :global([data-input="pointer"]) .row:focus:not(:focus-visible) {
    outline: none;
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
  .row.selected:hover .actions,
  .row.checked:hover .actions {
    background: var(--color-bg-selected);
  }
  .row.selectable.selected:not(.checked):hover .actions {
    background: var(--color-bg-hover);
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
