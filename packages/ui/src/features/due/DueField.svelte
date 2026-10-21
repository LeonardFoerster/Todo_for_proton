<script lang="ts">
  /**
   * Fälligkeit im Detail (P5-08): ein Button mit der Fälligkeit („Morgen 14:00“, Ton wie in der Zeile),
   * der `DuePicker` als Popover öffnet (`DuePopover`). Jede Wahl schließt das Popover; `Esc` und Klick
   * daneben auch, der Fokus geht zurück auf den Button.
   */
  import type { DueDto } from "@tasks/core-client";
  import { CalendarDays } from "../../icons";
  import { dueLabel, dueToInputDate, dueToInputTime } from "../../lib/format-date";
  import DuePopover from "./DuePopover.svelte";

  interface Props {
    value: DueDto | null;
    onchange: (due: DueDto | null) => void;
    id?: string;
  }

  let { value, onchange, id }: Props = $props();
  let open = $state(false);
  let button = $state<HTMLButtonElement>();

  const due = $derived(value ? dueLabel(value) : null);

  function close(refocus = true) {
    open = false;
    if (refocus) button?.focus();
  }

  function pick(next: DueDto | null) {
    onchange(next);
    close();
  }
</script>

<button
  bind:this={button}
  {id}
  type="button"
  class="due-field {due?.tone ?? 'empty'}"
  aria-label={due?.long ?? "Fällig: kein Datum"}
  aria-haspopup="dialog"
  aria-expanded={open}
  data-value={dueToInputDate(value)}
  data-time={dueToInputTime(value)}
  data-tooltip={due?.long}
  onclick={() => (open ? close() : (open = true))}
>
  <CalendarDays size={16} strokeWidth={1.75} aria-hidden="true" />
  <span>{due?.text ?? "Kein Datum"}</span>
</button>

{#if open && button}
  <DuePopover anchor={button} {value} onpick={pick} onclose={close} />
{/if}

<style>
  .due-field {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-height);
    padding: 0 var(--space-3) 0 var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--ease);
  }
  .due-field:hover,
  .due-field[aria-expanded="true"] {
    border-color: var(--color-accent);
  }
  .due-field :global(svg) {
    color: var(--color-text-muted);
  }
  .due-field.empty {
    color: var(--color-text-muted);
  }
  .due-field.overdue {
    color: var(--color-danger);
  }
  .due-field.today {
    color: var(--color-accent-text);
  }
</style>
