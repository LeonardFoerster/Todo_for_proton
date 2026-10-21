<script lang="ts">
  /**
   * Meta-Angaben einer Task in Zeile und Board-Karte (P5-03): Liste · Fälligkeit · Notiz, in beiden
   * gleich. Die Priorität steht nur im Ring des Hakens; Rot als Text heißt allein „überfällig“.
   * Serien (N4) tragen ein `repeat`-Icon; springt ihre Fälligkeit nach dem Abhaken, blendet der neue
   * Wert kurz ein (≤ 160 ms, nur `opacity`/`transform`).
   */
  import type { TaskSummaryDto } from "@tasks/core-client";
  import { untrack } from "svelte";
  import { Hash, Repeat, StickyNote } from "../../icons";
  import { trashedLabel, type DueLabel } from "../../lib/format-date";

  interface Props {
    task: TaskSummaryDto;
    due: DueLabel | null;
    /** Angezeigter Text der Fälligkeit (ganzes Datum oder nur Uhrzeit); leer = keine Anzeige. */
    dueText?: string | null;
    /** Listenname in Sammelansichten. */
    listName?: string;
    /** `card`: kleiner (Board). */
    size?: "row" | "card";
  }

  let { task, due, dueText = due?.text, listName, size = "row" }: Props = $props();
  /** Im Papierkorb statt der Fälligkeit: wann gelöscht (ADR-031). */
  const trashed = $derived(task.trashedAt ? trashedLabel(task.trashedAt) : null);

  const done = $derived(task.status === "done");
  const icon = $derived(size === "card" ? 12 : 14);
  // Nur echte Änderungen blenden ein – nicht das erste Zeichnen (virtuelle Liste beim Scrollen).
  // svelte-ignore state_referenced_locally
  let shown = dueText;
  let bump = $state(0);
  $effect(() => {
    const now = dueText;
    untrack(() => {
      if (now !== shown) {
        shown = now;
        bump++;
      }
    });
  });
</script>

<span class="meta {size}" class:done>
  {#if listName}<span class="list"><Hash size={12} strokeWidth={2} aria-hidden="true" />{listName}</span>{/if}
  {#if trashed}
    <span class="trashed" title={trashed.long}>{trashed.text}</span>
  {:else if due && dueText}
    {#key bump}<span class="due {due.tone}" class:bump={bump > 0} title={due.long}>{dueText}</span>{/key}
  {/if}
  {#if task.recurring && !trashed}
    <span class="repeat" title="Wiederholt sich" aria-label="wiederholt sich">
      <Repeat size={icon} strokeWidth={1.75} aria-hidden="true" />
    </span>
  {/if}
  {#if task.hasNotes}
    <span class="notes" title="Hat Notizen" aria-label="Hat Notizen">
      <StickyNote size={icon} strokeWidth={1.75} aria-hidden="true" />
    </span>
  {/if}
</span>

<style>
  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex: none;
    min-width: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .meta.card {
    gap: var(--space-2);
    font-size: var(--text-xs);
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
  .due.bump {
    animation: due-in var(--duration-fast) var(--ease);
  }
  @keyframes due-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .due.bump {
      animation: none;
    }
  }
  .done .due {
    color: var(--color-text-muted);
    font-weight: var(--weight-regular);
  }
  .notes,
  .repeat {
    display: grid;
    place-items: center;
  }
  .trashed {
    white-space: nowrap;
  }
</style>
