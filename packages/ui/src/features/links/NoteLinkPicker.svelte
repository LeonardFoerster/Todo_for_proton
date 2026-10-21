<script lang="ts">
  /**
   * Notiz verknüpfen (N9-02, ADR-036): Popover am Anker mit Suche über die Notizen (Name und Inhalt,
   * wie die Palette, entprellt) und „Neue Notiz“ mit dem Titel der Task. `Esc`/Klick daneben schließen.
   */
  import type { NoteHitDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import { FileCode, FileText, Plus } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import { POPOVER, showOnTop } from "../../lib/popover";

  interface Props {
    anchor: HTMLElement;
    /** Schon verknüpfte Notizen (ausgegraut). */
    linked: readonly string[];
    onpick: (noteId: string) => void;
    /** Neue Notiz anlegen und verknüpfen. */
    oncreate: () => void;
    onclose: (refocus: boolean) => void;
  }

  let { anchor, linked, onpick, oncreate, onclose }: Props = $props();
  const client = useCoreClient();
  const DEBOUNCE_MS = 100;
  let popover: HTMLDivElement;
  let input = $state<HTMLInputElement>();
  let query = $state("");
  let hits = $state.raw<NoteHitDto[]>([]);
  let pos = $state({ left: 0, top: 0 });
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;

  onMount(() => {
    showOnTop(popover);
    const a = anchor.getBoundingClientRect();
    const rect = popover.getBoundingClientRect();
    pos = {
      left: Math.max(8, Math.min(a.left, innerWidth - rect.width - 8)),
      top: Math.max(8, Math.min(a.bottom + 4, innerHeight - rect.height - 8)),
    };
    void tick().then(() => input?.focus());
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!popover.contains(target) && !anchor.contains(target)) onclose(false);
    };
    addEventListener("pointerdown", outside, true);
    return () => {
      clearTimeout(timer);
      removeEventListener("pointerdown", outside, true);
    };
  });

  function oninput(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value;
    clearTimeout(timer);
    if (!value.trim()) {
      hits = [];
      return;
    }
    timer = setTimeout(() => void search(value), DEBOUNCE_MS);
  }

  async function search(value: string) {
    const mine = ++request;
    const found = await client.notes.search(value, 20).catch(() => [] as NoteHitDto[]);
    if (mine === request) hits = found;
  }

  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onclose(true);
    } else if (e.key === "Enter" && e.target === input && !e.isComposing) {
      e.preventDefault();
      const first = hits.find((h) => !linked.includes(h.note.id));
      if (first) onpick(first.note.id);
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Tab") {
      const items = [input, ...popover.querySelectorAll<HTMLElement>("button:not(:disabled)")].filter(Boolean) as HTMLElement[];
      const i = items.indexOf(document.activeElement as HTMLElement);
      const back = e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey);
      e.preventDefault();
      items[(i + (back ? -1 : 1) + items.length) % items.length]?.focus();
    }
  }
</script>

<div
  bind:this={popover}
  class="popover"
  role="dialog"
  aria-label="Notiz verknüpfen"
  tabindex="-1"
  popover={POPOVER}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  {onkeydown}
>
  <input bind:this={input} bind:value={query} {oninput} class="search" placeholder="Notiz suchen …" aria-label="Notiz suchen" />
  <div class="items" role="group" aria-label="Notizen">
    {#each hits as hit (hit.note.id)}
      {@const Icon = hit.note.kind === "markdown" ? FileCode : FileText}
      <button type="button" class="item" disabled={linked.includes(hit.note.id)} onclick={() => onpick(hit.note.id)}>
        <Icon size={14} strokeWidth={1.75} aria-hidden="true" />
        <span class="name">{hit.note.name}</span>
        {#if hit.path.length > 0}<span class="path">{hit.path.join(" / ")}</span>{/if}
      </button>
    {/each}
    {#if query.trim() && hits.length === 0}<p class="empty">Keine Notiz gefunden.</p>{/if}
    <button type="button" class="item create" onclick={oncreate}>
      <Plus size={14} strokeWidth={2} aria-hidden="true" />Neue Notiz zu dieser Task
    </button>
  </div>
</div>

<style>
  .popover {
    position: fixed;
    z-index: 60;
    inset: auto;
    width: 300px;
    max-height: 340px;
    margin: 0;
    padding: var(--space-2);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg-raised);
    color: var(--color-text);
    box-shadow: var(--shadow-popover);
  }
  .search {
    height: 30px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
  }
  .search:focus {
    outline: none;
    border-color: var(--color-accent);
  }
  .items {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 30px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }
  .item:hover:not(:disabled),
  .item:focus-visible {
    outline: none;
    background: var(--color-bg-hover);
  }
  .item:disabled {
    color: var(--color-text-muted);
    cursor: default;
  }
  .item :global(svg) {
    flex: none;
    color: var(--color-text-muted);
  }
  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .path {
    margin-left: auto;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }
  .create {
    margin-top: var(--space-1);
    color: var(--color-accent-text);
  }
  .empty {
    margin: var(--space-1) var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
