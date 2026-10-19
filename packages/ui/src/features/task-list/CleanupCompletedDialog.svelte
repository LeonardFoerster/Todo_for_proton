<script lang="ts">
  /** „Erledigte älter als 30 Tage löschen“ (P4-06): nennt Anzahl und Stichtag, Rückgängig im Toast. */
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";

  interface Props {
    count: number;
    /** Abgehakt vor diesem Tag (lokal). */
    cutoff: Date;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { count, cutoff, onconfirm, onclose }: Props = $props();
  const day = $derived(new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "long", year: "numeric" }).format(cutoff));
  const tasks = $derived(count === 1 ? "1 erledigte Task" : `${count} erledigte Tasks`);
</script>

<Dialog title="Erledigte aufräumen?" {onclose} width={420}>
  <p><strong>{tasks}</strong>, vor dem {day} abgehakt, {count === 1 ? "wird" : "werden"} gelöscht – auf allen Geräten.</p>
  <p class="note">Direkt danach lässt sich das im Hinweis unten rückgängig machen.</p>
  <div class="actions">
    <Button variant="ghost" onclick={onclose}>Abbrechen</Button>
    <Button variant="danger" onclick={onconfirm}>{count === 1 ? "Task löschen" : `${count} Tasks löschen`}</Button>
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
