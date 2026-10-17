<script lang="ts">
  import Dialog from "../../components/Dialog.svelte";
  import { formatKeys, type Shortcut } from "../../lib/shortcuts";

  interface Props {
    shortcuts: readonly Shortcut[];
    onclose: () => void;
  }

  let { shortcuts, onclose }: Props = $props();

  /** Gruppen in Reihenfolge des ersten Auftretens. */
  const groups = $derived(
    [...new Set(shortcuts.map((s) => s.group ?? "Allgemein"))].map(
      (g) => [g, shortcuts.filter((s) => (s.group ?? "Allgemein") === g)] as const,
    ),
  );
</script>

<Dialog title="Tastaturkürzel" {onclose}>
  {#each groups as [group, items] (group)}
    <h3>{group}</h3>
    <dl>
      {#each items as s (s.description)}
        <dt>{s.description}</dt>
        <dd>
          {#each (s.label ?? s.keys.map(formatKeys).join(" / ")).split(" / ") as k, i (i)}
            {#if i > 0}<span class="or">oder</span>{/if}<kbd>{k}</kbd>
          {/each}
        </dd>
      {/each}
    </dl>
  {/each}
</Dialog>

<style>
  h3 {
    margin: var(--space-4) 0 var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  h3:first-child {
    margin-top: 0;
  }
  dl {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-2) var(--space-4);
    margin: 0;
  }
  dt {
    align-self: center;
  }
  dd {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-1);
    margin: 0;
  }
  kbd {
    min-width: 24px;
    padding: 2px 6px;
    border: 1px solid var(--color-border-strong);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    font: inherit;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    text-align: center;
  }
  .or {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }
</style>
