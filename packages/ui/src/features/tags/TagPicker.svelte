<script lang="ts">
  /**
   * Tags wählen (N8-03, ADR-035): Popover am Anker mit Suchfeld und den vorhandenen Tags als
   * Umschalter; `Enter` schaltet den ersten Treffer um oder nimmt einen neuen Namen. Arbeitet mit
   * Namen (Groß/Klein egal) – angelegt wird beim Aufrufer. `Esc` und Klick daneben schließen.
   */
  import type { TagDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import { Check, Plus } from "../../icons";
  import { POPOVER, showOnTop } from "../../lib/popover";

  interface Props {
    anchor: HTMLElement;
    tags: readonly TagDto[];
    /** Gewählte Namen. */
    selected: readonly string[];
    ontoggle: (name: string) => void;
    /** `refocus`: Fokus zurück auf den Anker (nicht bei Klick daneben). */
    onclose: (refocus: boolean) => void;
  }

  let { anchor, tags, selected, ontoggle, onclose }: Props = $props();
  let popover: HTMLDivElement;
  let input = $state<HTMLInputElement>();
  let query = $state("");
  let pos = $state({ left: 0, top: 0 });

  const key = (s: string) => s.toLowerCase();
  const isSelected = (name: string) => selected.some((s) => key(s) === key(name));
  const clean = $derived(query.trim().replace(/^#+/, "").split(/\s+/).filter(Boolean).join("-"));
  // Gewählte, die es (noch) nicht gibt, stehen mit in der Liste.
  const names = $derived([
    ...tags.map((t) => t.name),
    ...selected.filter((s) => !tags.some((t) => key(t.name) === key(s))),
  ]);
  const matches = $derived(names.filter((n) => key(n).includes(key(clean))));
  const canCreate = $derived(clean.length > 0 && clean.length <= 50 && !names.some((n) => key(n) === key(clean)));

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
    return () => removeEventListener("pointerdown", outside, true);
  });

  function toggle(name: string) {
    ontoggle(name);
    query = "";
    input?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onclose(true);
    } else if (e.key === "Enter" && e.target === input && !e.isComposing) {
      e.preventDefault();
      if (canCreate) toggle(clean);
      else if (matches[0]) toggle(matches[0]);
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const items = [input, ...popover.querySelectorAll<HTMLElement>("button")].filter(Boolean) as HTMLElement[];
      const i = items.indexOf(document.activeElement as HTMLElement);
      const next = items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length];
      e.preventDefault();
      next?.focus();
    } else if (e.key === "Tab") {
      // Im Popover bleiben.
      const items = [input, ...popover.querySelectorAll<HTMLElement>("button")].filter(Boolean) as HTMLElement[];
      const i = items.indexOf(document.activeElement as HTMLElement);
      const next = e.shiftKey ? (i <= 0 ? items.length - 1 : -1) : i === items.length - 1 ? 0 : -1;
      if (next >= 0) {
        e.preventDefault();
        items[next].focus();
      }
    }
  }
</script>

<div
  bind:this={popover}
  class="popover"
  role="dialog"
  aria-label="Tags wählen"
  tabindex="-1"
  popover={POPOVER}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  {onkeydown}
>
  <input bind:this={input} bind:value={query} class="search" placeholder="Tag suchen oder anlegen …" aria-label="Tag suchen" maxlength="51" />
  <div class="items" role="group" aria-label="Tags">
    {#each matches as name (name)}
      <button type="button" class="item" aria-pressed={isSelected(name)} onclick={() => toggle(name)}>
        <span class="check" aria-hidden="true">{#if isSelected(name)}<Check size={14} strokeWidth={2} />{/if}</span>
        <span class="hash" aria-hidden="true">#</span>{name}
      </button>
    {/each}
    {#if canCreate}
      <button type="button" class="item create" onclick={() => toggle(clean)}>
        <span class="check" aria-hidden="true"><Plus size={14} strokeWidth={2} /></span>Neuer Tag „{clean}“
      </button>
    {/if}
    {#if matches.length === 0 && !canCreate}
      <p class="empty">{tags.length === 0 ? "Noch keine Tags – Namen eintippen." : "Kein Treffer."}</p>
    {/if}
  </div>
</div>

<style>
  .popover {
    position: fixed;
    z-index: 60;
    inset: auto;
    width: 240px;
    max-height: 320px;
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
    height: 30px;
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
  .item:hover,
  .item:focus-visible {
    outline: none;
    background: var(--color-bg-hover);
  }
  .check {
    display: grid;
    place-items: center;
    width: 16px;
    color: var(--color-accent);
  }
  .hash {
    margin-right: -6px;
    color: var(--color-text-muted);
  }
  .create {
    color: var(--color-accent-text);
  }
  .empty {
    margin: var(--space-1) var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
