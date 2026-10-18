<script lang="ts">
  /**
   * Aktionsleiste des Auswahlmodus: ersetzt die Aktionen der Kopfzeile (gleiche Höhe, keine
   * zweite Zeile). Der Zähler ist eine Live-Region („3 ausgewählt“).
   */
  import type { Snippet } from "svelte";
  import Button from "../../components/Button.svelte";
  import SelectBox from "../../components/SelectBox.svelte";

  interface Props {
    /** Auswahl gemessen an der ganzen Ansicht. */
    all: "none" | "some" | "all";
    count: number;
    /** Alle auswählen bzw. (bei „all“) keine. */
    ontoggleall: () => void;
    ondone: () => void;
    /** Sammelaktionen (Erledigt, Verschieben, Löschen …). */
    actions?: Snippet;
  }

  let { all, count, ontoggleall, ondone, actions }: Props = $props();
</script>

<div class="bar" role="toolbar" aria-label="Auswahl">
  <SelectBox
    checked={all === "all" ? true : all === "some" ? "mixed" : false}
    label={all === "all" ? "Keine auswählen   Strg+A" : "Alle auswählen   Strg+A"}
    onchange={ontoggleall}
  />
  <span class="count" role="status">{count} ausgewählt</span>
  <span class="actions">
    {#if actions}{@render actions()}{/if}
    <Button variant="ghost" onclick={ondone} title="Auswahl beenden (Esc)">Fertig</Button>
  </span>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex: 1;
    min-width: 0;
    margin-left: var(--space-4);
  }
  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin-left: auto;
  }
</style>
