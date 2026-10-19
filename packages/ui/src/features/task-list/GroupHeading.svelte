<script lang="ts">
  /**
   * Abschnittsüberschrift in Sammelansichten (P4). Hat die Höhe einer Zeile (virtuelle Liste),
   * ist nicht fokussierbar und keine Option – ↑/↓ überspringen sie, weil die Tastatur über die
   * Tasks läuft. Optional eine Aktion für den ganzen Abschnitt (z. B. „Alle auf heute“).
   */
  import type { GroupHeader } from "../../lib/groups";

  interface Props {
    header: GroupHeader;
    actionLabel?: string;
    onaction?: () => void;
  }

  let { header, actionLabel, onaction }: Props = $props();
</script>

<div class="heading {header.group}" role="presentation">
  <span class="label">{header.label}</span>
  <span class="count">{header.count}</span>
  {#if actionLabel && onaction}
    <button type="button" class="action" onclick={onaction}>{actionLabel}</button>
  {/if}
</div>

<style>
  .heading {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-4) 6px;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .count {
    color: var(--color-text-muted);
    font-weight: var(--weight-regular);
    font-variant-numeric: tabular-nums;
  }
  .overdue .label {
    color: var(--color-danger);
  }
  .action {
    margin-left: auto;
    padding: 2px var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-accent-text);
    font: inherit;
    font-weight: var(--weight-medium);
    cursor: pointer;
  }
  .action:hover {
    background: var(--color-bg-hover);
  }
</style>
