<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";

  interface Props extends HTMLInputAttributes {
    value?: string;
    ref?: HTMLInputElement;
    /** Ohne Rahmen bis Hover/Fokus (z. B. Titel im Detail-Panel). */
    quiet?: boolean;
    invalid?: boolean;
  }

  let { value = $bindable(""), ref = $bindable(), quiet = false, invalid = false, type = "text", ...rest }: Props =
    $props();
</script>

<input bind:this={ref} bind:value {type} class="input" class:quiet aria-invalid={invalid || undefined} {...rest} />

<style>
  .input {
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    transition:
      border-color var(--duration-fast) var(--ease),
      box-shadow var(--duration-fast) var(--ease);
  }
  .input::placeholder {
    color: var(--color-text-muted);
  }
  .input:hover {
    border-color: var(--color-control-border);
  }
  .input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
  .input[aria-invalid="true"] {
    border-color: var(--color-danger);
  }
  .quiet {
    border-color: transparent;
    background: transparent;
  }
  .quiet:hover {
    border-color: var(--color-border-strong);
  }
  .input:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
