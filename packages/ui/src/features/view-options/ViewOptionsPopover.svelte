<script lang="ts">
  /**
   * Sortieren und Filtern einer Ansicht (N10-01, ADR-037): Popover am Kopfzeilen-Button mit drei
   * Gruppen als Optionsfelder – Sortierung, Priorität, Tag. Jede Wahl gilt sofort (`onchange`) und
   * wird pro Ansicht gemerkt. `Esc`/Klick daneben schließen.
   */
  import type { PriorityDto, TagDto, TaskSortDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import { POPOVER, showOnTop } from "../../lib/popover";
  import type { Route } from "../../lib/router.svelte";
  import { PRIORITY_LABELS, sortable, sortLabel, type ViewOptions } from "../../lib/view-options";

  interface Props {
    anchor: HTMLElement;
    route: Route;
    options: ViewOptions;
    tags: readonly TagDto[];
    onchange: (next: ViewOptions) => void;
    onclose: (refocus: boolean) => void;
  }

  let { anchor, route, options, tags, onchange, onclose }: Props = $props();
  let popover: HTMLDivElement;
  let pos = $state({ left: 0, top: 0 });
  const SORTS: TaskSortDto[] = ["view", "due", "priority", "title"];
  const PRIORITIES: PriorityDto[] = ["high", "medium", "low", "none"];

  onMount(() => {
    showOnTop(popover);
    const a = anchor.getBoundingClientRect();
    const rect = popover.getBoundingClientRect();
    pos = {
      left: Math.max(8, Math.min(a.right - rect.width, innerWidth - rect.width - 8)),
      top: Math.max(8, Math.min(a.bottom + 4, innerHeight - rect.height - 8)),
    };
    void tick().then(() => popover.querySelector<HTMLInputElement>("input:checked")?.focus());
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!popover.contains(target) && !anchor.contains(target)) onclose(false);
    };
    addEventListener("pointerdown", outside, true);
    return () => removeEventListener("pointerdown", outside, true);
  });

  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onclose(true);
    }
  }
</script>

<div
  bind:this={popover}
  class="popover"
  role="dialog"
  aria-label="Sortieren und filtern"
  tabindex="-1"
  popover={POPOVER}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  {onkeydown}
>
  {#if sortable(route)}
    <fieldset>
      <legend>Sortieren</legend>
      {#each SORTS as sort (sort)}
        <label>
          <input type="radio" name="sort" checked={options.sort === sort} onchange={() => onchange({ ...options, sort })} />
          {sort === "view" ? sortLabel("view", route) : sortLabel(sort, { view: "all" })}
        </label>
      {/each}
    </fieldset>
  {/if}
  <fieldset>
    <legend>Priorität</legend>
    <label>
      <input type="radio" name="priority" checked={options.priority === null} onchange={() => onchange({ ...options, priority: null })} />
      Alle
    </label>
    {#each PRIORITIES as priority (priority)}
      <label>
        <input
          type="radio"
          name="priority"
          checked={options.priority === priority}
          onchange={() => onchange({ ...options, priority })}
        />
        {PRIORITY_LABELS[priority]}
      </label>
    {/each}
  </fieldset>
  {#if route.view !== "tag" && tags.length > 0}
    <fieldset>
      <legend>Tag</legend>
      <label>
        <input type="radio" name="tag" checked={options.tag === null} onchange={() => onchange({ ...options, tag: null })} />
        Alle
      </label>
      {#each tags as tag (tag.id)}
        <label>
          <input type="radio" name="tag" checked={options.tag === tag.id} onchange={() => onchange({ ...options, tag: tag.id })} />
          #{tag.name}
        </label>
      {/each}
    </fieldset>
  {/if}
</div>

<style>
  .popover {
    position: fixed;
    z-index: 60;
    inset: auto;
    width: 220px;
    max-height: min(480px, calc(100vh - 32px));
    margin: 0;
    padding: var(--space-2) var(--space-3);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg-raised);
    color: var(--color-text);
    box-shadow: var(--shadow-popover);
  }
  .popover:focus {
    outline: none;
  }
  fieldset {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    margin-bottom: var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }
  label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 28px;
    font-size: var(--text-sm);
    cursor: pointer;
  }
  input {
    margin: 0;
    accent-color: var(--color-accent-fill);
  }
</style>
