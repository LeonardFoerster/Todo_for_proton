<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";
  import type { IconComponent } from "../icons";

  interface Props extends HTMLButtonAttributes {
    /** Zugänglicher Name und Tooltip – Pflicht. */
    label: string;
    icon: IconComponent;
    /** Shortcut im Tooltip, z. B. „N“. */
    shortcut?: string;
    size?: 28 | 32;
    /** Gedrückt-Zustand (Umschalter). */
    pressed?: boolean;
    tooltipSide?: "bottom" | "top" | "right";
  }

  let {
    label,
    icon: Icon,
    shortcut,
    size = 32,
    pressed,
    tooltipSide = "bottom",
    type = "button",
    ...rest
  }: Props = $props();
</script>

<button
  {type}
  class="icon-btn"
  style:--size="{size}px"
  aria-label={label}
  aria-pressed={pressed}
  data-tooltip={shortcut ? `${label}   ${shortcut}` : label}
  data-tooltip-side={tooltipSide === "bottom" ? undefined : tooltipSide}
  {...rest}
>
  <Icon size={size === 28 ? 16 : 18} strokeWidth={1.75} aria-hidden="true" />
</button>

<style>
  .icon-btn {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: var(--size);
    height: var(--size);
    padding: 0;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease),
      color var(--duration-fast) var(--ease);
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--color-bg-hover);
    color: var(--color-text);
  }
  .icon-btn[aria-pressed="true"] {
    background: var(--color-bg-selected);
    color: var(--color-accent-text);
  }
  .icon-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
