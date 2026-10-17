<script lang="ts">
  import { onMount } from "svelte";
  import { Hash } from "../../icons";

  interface Props {
    oncreate: (name: string) => Promise<void> | void;
    oncancel: () => void;
  }

  let { oncreate, oncancel }: Props = $props();
  let name = $state("");
  let input: HTMLInputElement;
  let busy = false;

  onMount(() => input.focus());

  async function submit() {
    const value = name.trim();
    if (!value) return oncancel();
    if (busy) return;
    busy = true;
    try {
      await oncreate(value);
    } finally {
      busy = false;
    }
  }
</script>

<div class="new-list">
  <Hash size={18} strokeWidth={1.75} aria-hidden="true" />
  <input
    bind:this={input}
    bind:value={name}
    maxlength="100"
    placeholder="Name der Liste"
    aria-label="Name der neuen Liste"
    onkeydown={(e) => {
      if (e.key === "Enter") void submit();
      if (e.key === "Escape") oncancel();
    }}
    onblur={() => (name.trim() ? void submit() : oncancel())}
  />
</div>

<style>
  .new-list {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 34px;
    padding: 0 var(--space-2) 0 10px;
    border-radius: var(--radius-md);
    background: var(--color-bg);
    box-shadow: 0 0 0 1px var(--color-accent), 0 0 0 4px var(--color-focus-ring);
    color: var(--color-text-muted);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--color-text);
    font: inherit;
  }
</style>
