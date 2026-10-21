<script lang="ts">
  /**
   * Meta-Angaben einer Task in Zeile und Board-Karte (P5-03): Liste · Fälligkeit · Notiz, in beiden
   * gleich. Die Priorität steht nur im Ring des Hakens; Rot als Text heißt allein „überfällig“.
   * Serien (N4) tragen ein `repeat`-Icon; springt ihre Fälligkeit nach dem Abhaken, blendet der neue
   * Wert kurz ein (≤ 160 ms, nur `opacity`/`transform`).
   */
  import type { TaskSummaryDto } from "@tasks/core-client";
  import { untrack } from "svelte";
  import { Hash, Link2, ListChecks, Repeat, StickyNote } from "../../icons";
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
    /** Namen der Tags (ADR-035), ohne `#`. */
    tags?: readonly string[];
  }

  let { task, due, dueText = due?.text, listName, size = "row", tags = [] }: Props = $props();
  /** Höchstens so viele Tags sichtbar, der Rest als „+n“. */
  const shownTags = $derived(tags.slice(0, size === "card" ? 2 : 3));
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
  {#if shownTags.length > 0}
    <span class="tags" title={tags.map((t) => `#${t}`).join(" ")}>
      {#each shownTags as tag (tag)}<span class="tag">#{tag}</span>{/each}
      {#if tags.length > shownTags.length}<span class="more">+{tags.length - shownTags.length}</span>{/if}
    </span>
  {/if}
  {#if task.recurring && !trashed}
    <span class="repeat" title="Wiederholt sich" aria-label="wiederholt sich">
      <Repeat size={icon} strokeWidth={1.75} aria-hidden="true" />
    </span>
  {/if}
  {#if task.checklistTotal > 0}
    <span
      class="checklist"
      class:complete={task.checklistDone === task.checklistTotal}
      title="Checkliste: {task.checklistDone} von {task.checklistTotal} erledigt"
      aria-label="Checkliste {task.checklistDone} von {task.checklistTotal}"
    >
      <ListChecks size={icon} strokeWidth={1.75} aria-hidden="true" />{task.checklistDone}/{task.checklistTotal}
    </span>
  {/if}
  {#if task.hasLinks}
    <span class="linked" title="Mit Notiz verknüpft" aria-label="mit Notiz verknüpft">
      <Link2 size={icon} strokeWidth={1.75} aria-hidden="true" />
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
  .tags {
    display: inline-flex;
    gap: var(--space-1);
    min-width: 0;
    overflow: hidden;
  }
  .tag {
    max-width: 96px;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    background: var(--color-bg-hover);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .more {
    white-space: nowrap;
  }
  .checklist {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .checklist.complete {
    color: var(--color-success);
  }
  .done .checklist {
    color: var(--color-text-muted);
  }
  .notes,
  .repeat,
  .linked {
    display: grid;
    place-items: center;
  }
  .trashed {
    white-space: nowrap;
  }
</style>
