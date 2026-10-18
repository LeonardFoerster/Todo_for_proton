<script lang="ts">
  import { onMount } from "svelte";
  import { Hash } from "../../icons";

  interface Props {
    /** Mit dem (getrimmten, nicht leeren) Namen; beim Umbenennen nur, wenn er sich ändert. */
    oncreate: (name: string) => Promise<void> | void;
    oncancel: () => void;
    /** Umbenennen: bisheriger Name (vorausgewählt). */
    initial?: string;
    label?: string;
  }

  let { oncreate, oncancel, initial = "", label = "Name der neuen Liste" }: Props = $props();
  // svelte-ignore state_referenced_locally
  let name = $state(initial);
  let input: HTMLInputElement;
  let busy = false;
  /** Abgebrochen oder gespeichert: Das `blur` beim Verschwinden darf nicht noch einmal speichern. */
  let finished = false;

  function cancel() {
    if (finished) return;
    finished = true;
    oncancel();
  }

  onMount(() => {
    input.focus();
    if (initial) input.select();
  });

  async function submit() {
    const value = name.trim();
    if (!value || value === initial) return cancel();
    if (busy || finished) return;
    busy = true;
    try {
      await oncreate(value);
      finished = true;
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
    aria-label={label}
    onkeydown={(e) => {
      if (e.key === "Enter") void submit();
      if (e.key === "Escape") cancel();
    }}
    onblur={() => (finished ? undefined : name.trim() ? void submit() : cancel())}
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
