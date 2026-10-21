<script lang="ts">
  /** Bestätigung „Endgültig löschen“ im Papierkorb (ADR-031) – nicht rückgängig zu machen. */
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";

  interface Props {
    /** „„Milch““ bzw. „3 Tasks“ bzw. „alle 12 Tasks im Papierkorb“. */
    subject: string;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { subject, onconfirm, onclose }: Props = $props();
</script>

<Dialog title="Endgültig löschen?" {onclose} width={420}>
  <p>{subject} {subject.startsWith("alle") ? "werden" : "wird"} auf allen Geräten gelöscht. Das lässt sich nicht rückgängig machen.</p>
  <div class="actions">
    <Button variant="ghost" onclick={onclose}>Abbrechen</Button>
    <Button
      variant="danger"
      onclick={() => {
        onconfirm();
        onclose();
      }}>Endgültig löschen</Button
    >
  </div>
</Dialog>

<style>
  p {
    margin: 0;
    line-height: 1.5;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-4);
  }
</style>
