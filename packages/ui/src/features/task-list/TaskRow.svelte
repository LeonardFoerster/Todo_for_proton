<script lang="ts">
  import type { TaskSummaryDto } from "@tasks/core-client";
  import Checkbox from "../../components/Checkbox.svelte";
  import SelectBox from "../../components/SelectBox.svelte";
  import { GripVertical, Trash } from "../../icons";
  import { dueLabel, dueToInputTime } from "../../lib/format-date";
  import type { DueMode } from "../../lib/groups";
  import { priorityInLabel, priorityLabel } from "./priority";
  import TaskMeta from "./TaskMeta.svelte";

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
  const priority = $derived(done ? "" : priorityLabel[task.priority]);
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
  aria-label={[task.title, done ? "erledigt" : null, due?.long, done ? null : priorityInLabel(task.priority)].filter(Boolean).join(", ")}
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
    tooltip={priority || undefined}
    onchange={(value) => ontoggle(task, value)}
  />
  <span class="title">{task.title}</span>
  <TaskMeta {task} {due} {dueText} {listName} />
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
    /* Lesespalte (P5-02): keine Trennlinien; Hover/Auswahl als gerundete Fläche mit Einzug. */
    margin-inline: calc(var(--content-inset) + var(--space-2));
    padding: 0 var(--space-4) 0 var(--space-6);
    border-radius: var(--radius-md);
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
    top: 8px;
    bottom: 8px;
    width: 3px;
    border-radius: 0 2px 2px 0;
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
  /* Tastatur-Umriss folgt der Rundung der Zeile (P5-04), nie eckig bis an den Rand. */
  .row:focus-visible,
  :global([data-input="keyboard"]) .row:focus {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: -2px;
    border-radius: var(--radius-md);
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
  .actions {
    position: absolute;
    right: var(--space-2);
    top: 50%;
    display: none;
    gap: 2px;
    padding-left: var(--space-2);
    border-radius: var(--radius-md);
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
