<script lang="ts">
  import type { HTMLTextareaAttributes } from "svelte/elements";

  interface Props extends HTMLTextareaAttributes {
    value?: string;
    ref?: HTMLTextAreaElement;
  }

  let { value = $bindable(""), ref = $bindable(), rows = 4, ...rest }: Props = $props();
</script>

<textarea bind:this={ref} bind:value {rows} class="textarea" {...rest}></textarea>

<style>
  .textarea {
    display: block;
    width: 100%;
    min-height: calc(var(--control-height) * 2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    line-height: 1.5;
    resize: vertical;
    transition:
      border-color var(--duration-fast) var(--ease),
      box-shadow var(--duration-fast) var(--ease);
  }
  .textarea::placeholder {
    color: var(--color-text-muted);
  }
  .textarea:hover {
    border-color: var(--color-control-border);
  }
  .textarea:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
</style>
