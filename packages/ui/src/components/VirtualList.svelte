<script lang="ts" generics="T">
  /**
   * Virtualisierte Liste mit fester Zeilenhöhe: rendert nur sichtbare Zeilen + Puffer.
   * `items` am besten als `$state.raw` halten (keine tiefe Reaktivität für große Listen).
   */
  import type { Snippet } from "svelte";

  interface Props {
    items: readonly T[];
    key: (item: T) => string;
    row: Snippet<[T, number]>;
    rowHeight?: number;
    overscan?: number;
    /** Nahe am Ende angekommen (z. B. nächste Seite laden). */
    onEndReached?: () => void;
    /** Inhalt unterhalb der Zeilen (z. B. Abschnitt „Erledigt“). */
    footer?: Snippet;
    label?: string;
    /** Über den Zeilen, im Koordinatensystem der Liste (z. B. Einfügelinie beim Ziehen). */
    overlay?: Snippet;
    onkeydown?: (e: KeyboardEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
    /** ARIA-Rolle des Containers (`list`, `listbox` oder `tree`). */
    role?: "list" | "listbox" | "tree";
    /** Listbox mit Mehrfachauswahl (`aria-multiselectable`). */
    multiselectable?: boolean;
  }

  let {
    items,
    key,
    row,
    rowHeight = 36,
    overscan = 8,
    onEndReached,
    footer,
    overlay,
    onkeydown,
    oncontextmenu,
    label,
    role = "list",
    multiselectable = false,
  }: Props = $props();

  let viewport: HTMLDivElement;
  let scrollTop = $state(0);
  let height = $state(0);

  const first = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const last = $derived(Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + overscan));
  const visible = $derived(items.slice(first, last));

  $effect(() => {
    if (onEndReached && last >= items.length - overscan && items.length > 0) onEndReached();
  });

  export function element(): HTMLDivElement {
    return viewport;
  }

  /** Scrollt so wenig wie nötig, damit Zeile `index` sichtbar ist. */
  export function scrollToIndex(index: number) {
    if (!viewport) return;
    const top = index * rowHeight;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + rowHeight > viewport.scrollTop + viewport.clientHeight) {
      viewport.scrollTop = top + rowHeight - viewport.clientHeight;
    }
  }
</script>

<div
  class="viewport"
  bind:this={viewport}
  bind:clientHeight={height}
  onscroll={() => (scrollTop = viewport.scrollTop)}
  {onkeydown}
  {oncontextmenu}
  {role}
  aria-label={label}
  aria-multiselectable={role === "listbox" && multiselectable ? true : undefined}
  tabindex="-1"
>
  <div class="spacer" style:height="{items.length * rowHeight}px">
    <div class="window" style:transform="translateY({first * rowHeight}px)">
      {#each visible as item, i (key(item))}
        {@render row(item, first + i)}
      {/each}
    </div>
    {#if overlay}{@render overlay()}{/if}
  </div>
  {#if footer}{@render footer()}{/if}
</div>

<style>
  .viewport {
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
    contain: strict;
  }
  .spacer {
    position: relative;
  }
  .window {
    position: absolute;
    inset: 0 0 auto 0;
    will-change: transform;
  }
</style>
