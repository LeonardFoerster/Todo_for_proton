<script lang="ts">
  /**
   * Fälligkeit im Detail (P5-08): ein Button mit der Fälligkeit („Morgen 14:00“, Ton wie in der Zeile),
   * der `DuePicker` als Popover öffnet. Jede Wahl schließt das Popover; `Esc` und Klick daneben auch,
   * der Fokus geht zurück auf den Button.
   */
  import type { DueDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import { CalendarDays } from "../../icons";
  import { dueLabel, dueToInputDate, dueToInputTime } from "../../lib/format-date";
  import DuePicker from "./DuePicker.svelte";

  interface Props {
    value: DueDto | null;
    onchange: (due: DueDto | null) => void;
    id?: string;
  }

  let { value, onchange, id }: Props = $props();
  let open = $state(false);
  let button: HTMLButtonElement;
  let popover = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0 });

  const due = $derived(value ? dueLabel(value) : null);

  async function show() {
    open = true;
    await tick();
    if (!popover) return;
    // Unter dem Button; passt es nicht ganz, so weit nach oben wie nötig (im Fenster gehalten).
    const anchor = button.getBoundingClientRect();
    const rect = popover.getBoundingClientRect();
    pos = {
      left: Math.max(8, Math.min(anchor.left, innerWidth - rect.width - 8)),
      top: Math.max(8, Math.min(anchor.bottom + 4, innerHeight - rect.height - 8)),
    };
  }

  function close(focus = true) {
    open = false;
    if (focus) button.focus();
  }

  function pick(next: DueDto | null) {
    onchange(next);
    close();
  }

  onMount(() => {
    const outside = (e: PointerEvent) => {
      if (open && popover && !popover.contains(e.target as Node) && !button.contains(e.target as Node)) close(false);
    };
    addEventListener("pointerdown", outside, true);
    return () => removeEventListener("pointerdown", outside, true);
  });

  /** Tasten bleiben im Popover (keine globalen Kürzel); Esc schließt, Tab läuft im Kreis. */
  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Tab" && popover) {
      const items = [...popover.querySelectorAll<HTMLElement>("input, button:not(:disabled):not([tabindex='-1'])")];
      const i = items.indexOf(document.activeElement as HTMLElement);
      const next = e.shiftKey ? (i <= 0 ? items.length - 1 : -1) : i === items.length - 1 ? 0 : -1;
      if (next >= 0) {
        e.preventDefault();
        items[next].focus();
      }
    }
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
  onclick={() => (open ? close() : void show())}
>
  <CalendarDays size={16} strokeWidth={1.75} aria-hidden="true" />
  <span>{due?.text ?? "Kein Datum"}</span>
</button>

{#if open}
  <div
    bind:this={popover}
    class="popover"
    role="dialog"
    aria-label="Fälligkeit wählen"
    tabindex="-1"
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    {onkeydown}
  >
    <DuePicker {value} onchange={pick} autofocus />
  </div>
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
  .popover {
    position: fixed;
    z-index: 60;
    width: 296px;
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg-raised);
    box-shadow: var(--shadow-popover);
  }
  .popover:focus {
    outline: none;
  }
</style>
