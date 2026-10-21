<script lang="ts">
  import { ChevronDown, ChevronRight, FileCode, FileText, Folder, FolderOpen } from "../../icons";
  import type { TreeRow } from "./tree-model";

  interface Props {
    row: TreeRow;
    /** Tastatur-Cursor (fokussierbare Zeile). */
    cursor: boolean;
    /** Offene Notiz (aktive Navigation). */
    active: boolean;
    renaming: boolean;
    dragging: boolean;
    /** Ziel beim Ziehen „hinein". */
    dropInto: boolean;
    onactivate: (row: TreeRow, e: MouseEvent) => void;
    onrename: (row: TreeRow) => void;
    onsubmitname: (row: TreeRow, name: string) => void;
    oncancelname: () => void;
    oncontextmenu: (row: TreeRow, e: MouseEvent) => void;
    onpointerdown: (row: TreeRow, e: PointerEvent) => void;
    /** Zeile hat den Fokus bekommen (Tab, Klick, Pfeile) → Cursor. */
    onfocus: (row: TreeRow) => void;
  }

  let {
    row,
    cursor,
    active,
    renaming,
    dragging,
    dropInto,
    onactivate,
    onrename,
    onsubmitname,
    oncancelname,
    oncontextmenu,
    onpointerdown,
    onfocus,
  }: Props = $props();

  const folder = $derived(row.item.type === "folder");
  const kindLabel = $derived(row.item.type === "note" ? (row.item.kind === "markdown" ? ".md" : ".txt") : "");
  let input = $state<HTMLInputElement>();

  $effect(() => {
    if (renaming && input) {
      input.focus();
      input.select();
    }
  });

  function keydown(e: KeyboardEvent) {
    // Nur das Namensfeld: Enter speichert, Esc bricht ab (Rest macht die Registry).
    if (e.key === "Enter") {
      e.preventDefault();
      onsubmitname(row, input?.value ?? "");
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      oncancelname();
    }
  }
</script>

<!-- Tasten (Pfeile, Enter, F2, Entf …) behandelt die zentrale Registry (notes-shortcuts.ts). -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="row"
  class:cursor
  class:active
  class:dragging
  class:drop-into={dropInto}
  role="treeitem"
  aria-level={row.depth + 1}
  aria-expanded={folder ? row.expanded : undefined}
  aria-selected={active}
  aria-label={folder ? row.item.name : `${row.item.name}${kindLabel}`}
  tabindex={cursor ? 0 : -1}
  data-tree-key={row.key}
  style:--depth={row.depth}
  onclick={(e) => {
    if (renaming) return;
    e.currentTarget.focus({ preventScroll: true });
    onactivate(row, e);
  }}
  ondblclick={() => !renaming && onrename(row)}
  oncontextmenu={(e) => {
    e.preventDefault();
    oncontextmenu(row, e);
  }}
  onpointerdown={(e) => !renaming && onpointerdown(row, e)}
  onfocus={(e) => e.target === e.currentTarget && onfocus(row)}
>
  <span class="twisty" aria-hidden="true">
    {#if folder}
      {#if row.expanded}<ChevronDown size={14} strokeWidth={1.75} />{:else}<ChevronRight size={14} strokeWidth={1.75} />{/if}
    {/if}
  </span>
  <span class="icon" aria-hidden="true">
    {#if folder}
      {#if row.expanded}<FolderOpen size={16} strokeWidth={1.75} />{:else}<Folder size={16} strokeWidth={1.75} />{/if}
    {:else if row.item.type === "note" && row.item.kind === "markdown"}
      <FileCode size={16} strokeWidth={1.75} />
    {:else}
      <FileText size={16} strokeWidth={1.75} />
    {/if}
  </span>
  {#if renaming}
    <input
      bind:this={input}
      class="name-input"
      aria-label={folder ? "Neuer Name des Ordners" : "Neuer Name der Notiz"}
      value={row.item.name}
      onkeydown={keydown}
      onblur={() => oncancelname()}
      spellcheck="false"
    />
  {:else}
    <span class="name">{row.item.name}</span>
  {/if}
</div>

<style>
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 32px;
    margin: 0 var(--space-2);
    padding: 0 var(--space-2) 0 calc(4px + var(--depth) * 16px);
    border-radius: var(--radius-md);
    color: var(--color-text);
    font-size: var(--text-md);
    cursor: default;
    user-select: none;
    touch-action: none;
  }
  .row:hover {
    background: var(--color-bg-hover);
  }
  .row.active {
    background: var(--color-bg-selected);
    color: var(--color-accent-text);
    font-weight: var(--weight-medium);
  }
  .row.dragging {
    opacity: 0.5;
  }
  .row.drop-into {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }
  .row:focus-visible,
  :global([data-input="keyboard"]) .row:focus {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: -2px;
  }
  :global([data-input="pointer"]) .row:focus:not(:focus-visible) {
    outline: none;
  }
  .twisty {
    display: grid;
    place-items: center;
    width: 16px;
    flex: none;
    color: var(--color-text-muted);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 20px;
    flex: none;
    color: var(--color-text-muted);
  }
  .active .icon {
    color: inherit;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .name-input {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 var(--space-1);
    border: 1px solid var(--color-accent);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }
</style>
