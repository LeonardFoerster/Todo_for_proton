<script lang="ts">
  /**
   * Namen einer Liste erfragen: Umbenennen, wenn die Seitenleiste eingeklappt/verborgen ist (sonst
   * inline dort), und „Neue Liste…“ im Listen-Picker.
   */
  import { onMount } from "svelte";
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";

  interface Props {
    title: string;
    submitLabel: string;
    /** Bisheriger Name (Umbenennen); leer = neue Liste. */
    initial?: string;
    onsubmit: (name: string) => Promise<void>;
    onclose: () => void;
  }

  let { title, submitLabel, initial = "", onsubmit, onclose }: Props = $props();
  // svelte-ignore state_referenced_locally
  let name = $state(initial);
  let error = $state<string | null>(null);
  let input: HTMLInputElement;

  onMount(() => {
    input.focus();
    if (initial) input.select();
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    const value = name.trim();
    if (!value) {
      error = "Der Name darf nicht leer sein.";
      return;
    }
    if (value === initial) return onclose();
    try {
      await onsubmit(value);
      onclose();
    } catch (err) {
      error = (err as Error).message;
    }
  }
</script>

<Dialog {title} {onclose} width={400}>
  <form onsubmit={submit}>
    <input bind:this={input} bind:value={name} maxlength="100" aria-label={initial ? "Neuer Name der Liste" : "Name der neuen Liste"} />
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <Button variant="ghost" onclick={onclose}>Abbrechen</Button>
      <Button variant="primary" type="submit">{submitLabel}</Button>
    </div>
  </form>
</Dialog>

<style>
  input {
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }
  input:focus {
    outline: 2px solid var(--color-focus-ring);
    border-color: var(--color-accent);
  }
  .error {
    margin: var(--space-2) 0 0;
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-4);
  }
</style>
