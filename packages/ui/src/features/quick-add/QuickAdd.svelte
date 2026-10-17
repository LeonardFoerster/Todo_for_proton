<script lang="ts">
  import type { DueDto } from "@tasks/core-client";
  import { toasts } from "../../components/toasts.svelte";
  import { CalendarDays, Plus } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import { dueLabel } from "../../lib/format-date";
  import { parseQuickAdd } from "./parse";

  interface Props {
    /** Zielliste (`undefined` = Eingang). */
    listId?: string;
    /** Vorgabe ohne Schlüsselwort (z. B. „heute“ in der Heute-Ansicht). */
    defaultDue?: DueDto | null;
    placeholder?: string;
  }

  let { listId, defaultDue = null, placeholder = "Task hinzufügen" }: Props = $props();

  const client = useCoreClient();
  let value = $state("");
  let input: HTMLInputElement;
  let busy = false;

  const parsed = $derived(parseQuickAdd(value));
  const due = $derived(parsed.due ?? (value.trim() ? defaultDue : null));
  const preview = $derived(due ? dueLabel(due) : null);

  export function focus() {
    input?.focus();
  }

  async function submit() {
    const { title } = parsed;
    if (!title || busy) return;
    busy = true;
    const dueToSet = due;
    value = "";
    try {
      const task = await client.createTask({ title, listId });
      if (dueToSet) await client.updateTask(task.id, { due: dueToSet });
    } catch (e) {
      value = title;
      toasts.show({ message: `Nicht angelegt: ${(e as Error).message}`, tone: "danger" });
    } finally {
      busy = false;
      input.focus();
    }
  }
</script>

<div class="quick-add">
  <span class="plus" aria-hidden="true"><Plus size={18} strokeWidth={2} /></span>
  <input
    bind:this={input}
    bind:value
    {placeholder}
    aria-label="Neue Task"
    maxlength="500"
    autocomplete="off"
    spellcheck="true"
    onkeydown={(e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        void submit();
      } else if (e.key === "Escape") {
        value = "";
        input.blur();
      }
    }}
  />
  {#if preview}
    <span class="chip {preview.tone}" title={preview.long}>
      <CalendarDays size={14} strokeWidth={1.75} aria-hidden="true" />{preview.text}
    </span>
  {/if}
  {#if value.trim()}<kbd>Enter</kbd>{/if}
</div>

<style>
  .quick-add {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 44px;
    margin: 0 var(--space-4) var(--space-2);
    padding: 0 var(--space-3) 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    transition:
      border-color var(--duration-fast) var(--ease),
      box-shadow var(--duration-fast) var(--ease);
  }
  .quick-add:hover {
    border-color: var(--color-border-strong);
  }
  .quick-add:focus-within {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
  .plus {
    display: grid;
    place-items: center;
    width: 20px;
    color: var(--color-accent);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--color-text);
    font: inherit;
  }
  input::placeholder {
    color: var(--color-text-muted);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-full);
    background: var(--color-bg-selected);
    color: var(--color-accent-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    white-space: nowrap;
  }
  kbd {
    padding: 1px 6px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-xs);
  }
</style>
