<script lang="ts">
  /** Ergebnis von „Ordner importieren…" (`notes.md` §8): Zahlen und aufklappbare Gründe. */
  import type { ImportReportDto } from "@tasks/core-client";
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";

  interface Props {
    report: ImportReportDto;
    onclose: () => void;
  }

  let { report, onclose }: Props = $props();

  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
</script>

<Dialog title="Ordner importiert" {onclose} width={480}>
  <p class="summary">
    {plural(report.notes, "Notiz", "Notizen")} in {plural(report.folders + 1, "Ordner", "Ordnern")} importiert{report.skipped
      .length > 0
      ? `, ${report.skipped.length} übersprungen`
      : ""}.
  </p>
  {#if report.skipped.length > 0}
    <details>
      <summary>Übersprungen ({report.skipped.length})</summary>
      <ul>
        {#each report.skipped as skip, i (i)}
          <li><span class="path">{skip.path}</span> <span class="reason">– {skip.reason}</span></li>
        {/each}
      </ul>
    </details>
  {/if}
  <div class="actions">
    <Button variant="primary" onclick={onclose}>Fertig</Button>
  </div>
</Dialog>

<style>
  .summary {
    margin: 0 0 var(--space-3);
    line-height: 1.5;
  }
  details {
    margin-bottom: var(--space-3);
  }
  summary {
    cursor: pointer;
    color: var(--color-text-muted);
  }
  ul {
    max-height: 240px;
    margin: var(--space-2) 0 0;
    padding-left: var(--space-4);
    overflow-y: auto;
    font-size: var(--text-sm);
  }
  .reason {
    color: var(--color-text-muted);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
