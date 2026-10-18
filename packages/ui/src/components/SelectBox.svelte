<script lang="ts">
  /**
   * Eckiges Auswahlkästchen (Mehrfachauswahl) – bewusst anders als der runde Erledigt-Haken.
   * Ohne `onchange` rein dekorativ (die Zeile trägt `aria-selected`), sonst ein
   * `<button role="checkbox">` mit Tri-State (`"mixed"`).
   */
  interface Props {
    checked: boolean | "mixed";
    /** Zugänglicher Name (nur interaktiv). */
    label?: string;
    onchange?: () => void;
    disabled?: boolean;
  }

  let { checked, label, onchange, disabled = false }: Props = $props();
</script>

{#snippet box()}
  <svg viewBox="0 0 16 16" aria-hidden="true">
    <rect class="frame" x="1.75" y="1.75" width="12.5" height="12.5" rx="3" />
    {#if checked === "mixed"}
      <path class="mark" d="M5 8h6" />
    {:else if checked}
      <path class="mark" d="M4.6 8.2l2.3 2.3 4.5-4.9" />
    {/if}
  </svg>
{/snippet}

{#if onchange}
  <button
    type="button"
    role="checkbox"
    class="select"
    class:on={checked !== false}
    aria-checked={checked}
    aria-label={label}
    data-tooltip={label}
    {disabled}
    onclick={(e) => {
      e.stopPropagation();
      onchange();
    }}
  >
    {@render box()}
  </button>
{:else}
  <span class="select" class:on={checked !== false} aria-hidden="true">{@render box()}</span>
{/if}

<style>
  .select {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 24px;
    height: 24px;
    margin: -4px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-on-accent);
  }
  button.select {
    cursor: pointer;
  }
  button.select:hover:not(:disabled) {
    background: var(--color-bg-hover);
  }
  button.select:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  svg {
    width: 16px;
    height: 16px;
  }
  .frame {
    fill: transparent;
    stroke: var(--color-control-border);
    stroke-width: 1.5;
  }
  .on .frame {
    fill: var(--color-accent-fill);
    stroke: var(--color-accent-fill);
  }
  .mark {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.75;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
