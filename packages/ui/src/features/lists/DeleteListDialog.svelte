<script lang="ts">
  /** Bestätigung vor dem Löschen einer Liste (destruktiv, ui-guidelines §4): nennt, was mit den Tasks passiert. */
  import type { TaskListDto } from "@tasks/core-client";
  import { onMount } from "svelte";
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import { formatCount, type ListsState } from "../../lib/lists.svelte";

  interface Props {
    list: TaskListDto;
    lists: ListsState;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { list, lists, onconfirm, onclose }: Props = $props();
  let counts = $state<{ open: number; done: number } | null>(null);

  onMount(() => {
    void lists.countTasks(list.id).then((c) => (counts = c));
  });

  const tasks = (n: number) => (n === 1 ? "1 Task" : `${formatCount(n)} Tasks`);
</script>

<Dialog title="Liste löschen?" {onclose} width={420}>
  <p>
    Die Liste <strong>„{list.name}“</strong> wird gelöscht.
    {#if counts === null}
      Ihre Tasks wandern in den Eingang.
    {:else if counts.open + counts.done === 0}
      Sie enthält keine Tasks.
    {:else}
      Ihre {tasks(counts.open)} offen{counts.open === 1 ? "e" : "en"} und {tasks(counts.done)} erledigt{counts.done === 1 ? "e" : "en"}
      wandern in den Eingang.
    {/if}
  </p>
  <p class="note">Das lässt sich nicht rückgängig machen.</p>
  <div class="actions">
    <Button variant="ghost" onclick={onclose}>Abbrechen</Button>
    <Button variant="danger" onclick={onconfirm}>Liste löschen</Button>
  </div>
</Dialog>

<style>
  p {
    margin: 0 0 var(--space-3);
    line-height: 1.5;
  }
  .note {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-4);
  }
</style>
