<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import type { IconComponent } from "../icons";

  interface Props extends HTMLButtonAttributes {
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "md" | "lg";
    icon?: IconComponent;
    /** Volle Breite (z. B. Primärbutton in der Sidebar). */
    block?: boolean;
    children?: Snippet;
  }

  let { variant = "secondary", size = "md", icon: Icon, block = false, type = "button", children, ...rest }: Props =
    $props();
</script>

<button {type} class="btn {variant} {size}" class:block {...rest}>
  {#if Icon}<Icon size={size === "lg" ? 18 : 16} strokeWidth={1.75} aria-hidden="true" />{/if}
  {#if children}<span class="label">{@render children()}</span>{/if}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font: inherit;
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease),
      border-color var(--duration-fast) var(--ease),
      color var(--duration-fast) var(--ease);
  }
  .lg {
    height: var(--control-height-lg);
    padding: 0 var(--space-4);
  }
  .block {
    width: 100%;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .primary {
    background: var(--color-accent-fill);
    color: var(--color-on-accent);
  }
  .primary:hover:not(:disabled) {
    background: var(--color-accent-fill-hover);
  }

  .secondary {
    background: var(--color-bg);
    border-color: var(--color-border-strong);
    color: var(--color-text);
  }
  .secondary:hover:not(:disabled) {
    background: var(--color-bg-hover);
  }

  .ghost {
    background: transparent;
    color: var(--color-text);
  }
  .ghost:hover:not(:disabled) {
    background: var(--color-bg-hover);
  }

  .danger {
    background: var(--color-danger-fill);
    color: var(--color-on-accent);
  }
  .danger:hover:not(:disabled) {
    background: var(--color-danger-fill-hover);
  }

  .btn:active:not(:disabled) {
    transform: translateY(0.5px);
  }
  .btn:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
</style>
