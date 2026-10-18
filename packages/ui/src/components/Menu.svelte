<script lang="ts" module>
  import type { IconComponent } from "../icons";

  export interface MenuItem {
    /** Eindeutiger Schlüssel, falls Beschriftungen doppelt vorkommen können (z. B. Listennamen). */
    id?: string;
    label: string;
    icon?: IconComponent;
    /** Rechtsbündig gedimmt, z. B. „Entf“. */
    shortcut?: string;
    danger?: boolean;
    disabled?: boolean;
    action: () => void;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";

  interface Props {
    items: MenuItem[];
    /** Position (Viewport-Koordinaten, z. B. vom Rechtsklick). */
    x: number;
    y: number;
    label: string;
    onclose: () => void;
  }

  let { items, x, y, label, onclose }: Props = $props();
  let menu: HTMLDivElement;
  let left = $state(0);
  let top = $state(0);

  const enabled = () => [...menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')];

  onMount(() => {
    // Im Fenster halten.
    const rect = menu.getBoundingClientRect();
    left = Math.min(x, innerWidth - rect.width - 8);
    top = Math.min(y, innerHeight - rect.height - 8);
    enabled()[0]?.focus();
    const outside = (e: PointerEvent) => {
      if (!menu.contains(e.target as Node)) onclose();
    };
    const timer = setTimeout(() => addEventListener("pointerdown", outside, true));
    addEventListener("blur", onclose);
    return () => {
      clearTimeout(timer);
      removeEventListener("pointerdown", outside, true);
      removeEventListener("blur", onclose);
    };
  });

  function onkeydown(e: KeyboardEvent) {
    const list = enabled();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = (i + (e.key === "ArrowDown" ? 1 : -1) + list.length) % list.length;
      list[next]?.focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      list[e.key === "Home" ? 0 : list.length - 1]?.focus();
    } else if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      onclose();
    }
    e.stopPropagation();
  }
</script>

<div
  bind:this={menu}
  class="menu"
  role="menu"
  tabindex="-1"
  aria-label={label}
  style:left="{left}px"
  style:top="{top}px"
  {onkeydown}
>
  {#each items as item (item.id ?? item.label)}
    <button
      type="button"
      role="menuitem"
      class:danger={item.danger}
      disabled={item.disabled}
      onclick={() => {
        onclose();
        item.action();
      }}
    >
      <span class="icon">{#if item.icon}<item.icon size={16} strokeWidth={1.75} aria-hidden="true" />{/if}</span>
      <span class="label">{item.label}</span>
      {#if item.shortcut}<kbd>{item.shortcut}</kbd>{/if}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 220px;
    padding: var(--space-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-raised);
    box-shadow: var(--shadow-popover);
    animation: pop var(--duration-fast) var(--ease);
  }
  button {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 32px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--color-bg-hover);
  }
  /* Tastaturfokus deutlich: Fläche + Ring (die Hover-Fläche allein hat zu wenig Kontrast). */
  button:focus-visible {
    outline: 2px solid var(--color-focus-ring);
    outline-offset: -2px;
    background: var(--color-bg-hover);
  }
  button:disabled {
    color: var(--color-text-disabled);
    cursor: default;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 18px;
    color: var(--color-text-muted);
  }
  .label {
    flex: 1;
  }
  .danger,
  .danger .icon {
    color: var(--color-danger);
  }
  kbd {
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-xs);
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }
</style>
