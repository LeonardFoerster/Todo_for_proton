<script lang="ts">
  /**
   * „Datum wählen…“ (P4-04): Text wie in der Titelzeile von „Neue Task“ – „fr“, „24.10.“,
   * „morgen 14:00“ – über denselben Parser wie dort (`parseDue`), kein zweiter.
   */
  import type { DueDto } from "@tasks/core-client";
  import { onMount } from "svelte";
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import { dueLabel } from "../../lib/format-date";
  import { parseDue } from "../new-task/parse";

  interface Props {
    /** Für den Titel, z. B. „3 Tasks“. */
    subject: string;
    onsubmit: (due: DueDto) => void;
    onclose: () => void;
  }

  let { subject, onsubmit, onclose }: Props = $props();
  let text = $state("");
  let error = $state<string | null>(null);
  let input: HTMLInputElement;

  const parsed = $derived(text.trim() ? parseDue(text) : null);
  const preview = $derived(parsed ? dueLabel(parsed).long : null);

  onMount(() => input.focus());

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!parsed) {
      error = "Kein Datum erkannt. Beispiele: fr, 24.10., morgen 14:00";
      return;
    }
    onsubmit(parsed);
    onclose();
  }
</script>

<Dialog title="Fälligkeit für {subject}" {onclose} width={400}>
  <form onsubmit={submit}>
    <input
      bind:this={input}
      bind:value={text}
      aria-label="Fällig am"
      aria-describedby="due-prompt-hint"
      aria-invalid={error ? true : undefined}
      placeholder="fr, 24.10., morgen 14:00"
      oninput={() => (error = null)}
    />
    <p class="hint" id="due-prompt-hint" aria-live="polite">{preview ?? "Wochentag, Datum, „morgen“, optional mit Uhrzeit"}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <Button variant="ghost" onclick={onclose}>Abbrechen</Button>
      <Button variant="primary" type="submit">Fälligkeit setzen</Button>
    </div>
  </form>
</Dialog>

<style>
  input {
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }
  input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
  .hint {
    margin: var(--space-2) 0 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
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
