<script lang="ts">
  /**
   * Runder Task-Haken: Hover deutet den Haken an, Abhaken füllt den Ring und zeichnet das Häkchen
   * (≤ 160 ms, nur transform/opacity/stroke). Ein `<button role="checkbox">`, damit Leertaste/Enter
   * ohne Formular funktionieren.
   */
  interface Props {
    checked: boolean;
    /** Zugänglicher Name, z. B. „Erledigt: Milch kaufen“. */
    label: string;
    onchange?: (checked: boolean) => void;
    disabled?: boolean;
    /** Ringfarbe für Priorität (nur offen sichtbar). */
    tone?: "default" | "high" | "medium" | "low";
    tabindex?: number;
    /** Tooltip, z. B. die Priorität (sie steht nur im Ring). */
    tooltip?: string;
  }

  let { checked, label, onchange, disabled = false, tone = "default", tabindex, tooltip }: Props = $props();
</script>

<button
  type="button"
  role="checkbox"
  class="check {tone}"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  {tabindex}
  data-tooltip={tooltip}
  onclick={(e) => {
    e.stopPropagation();
    onchange?.(!checked);
  }}
>
  <svg viewBox="0 0 20 20" aria-hidden="true">
    <circle class="ring" cx="10" cy="10" r="8.25" />
    <circle class="fill" cx="10" cy="10" r="9" />
    <path class="tick" d="M6.2 10.4l2.5 2.5 5.1-5.6" pathLength="1" />
  </svg>
</button>

<style>
  .check {
    --ring: var(--color-control-border);
    display: inline-grid;
    place-items: center;
    flex: none;
    /* 28 px Trefferfläche, optisch 20 px (negativer Rand hält das Zeilenlayout). */
    width: 28px;
    height: 28px;
    margin: -4px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-full);
    background: transparent;
    cursor: pointer;
  }
  .high {
    --ring: var(--color-danger);
  }
  .medium {
    --ring: var(--color-warning);
  }
  .low {
    --ring: var(--color-accent);
  }
  /* Priorität: zarte Füllung im Ring, damit sie ohne Fähnchen auffällt (P5-03). */
  .high .ring,
  .medium .ring,
  .low .ring {
    fill: color-mix(in srgb, var(--ring) 12%, transparent);
    stroke-width: 1.75;
  }
  svg {
    width: 20px;
    height: 20px;
    overflow: visible;
  }
  .ring {
    fill: transparent;
    stroke: var(--ring);
    stroke-width: 1.5;
    transition: stroke var(--duration-fast) var(--ease);
  }
  .fill {
    fill: var(--color-accent-fill);
    transform-origin: 10px 10px;
    transform: scale(0.4);
    opacity: 0;
    transition:
      transform var(--duration-base) var(--ease),
      opacity var(--duration-fast) var(--ease);
  }
  .tick {
    fill: none;
    stroke: var(--color-text-muted);
    stroke-width: 1.75;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    opacity: 0;
    transition:
      stroke-dashoffset var(--duration-base) var(--ease),
      opacity var(--duration-fast) var(--ease);
  }

  /* Hover: Haken andeuten */
  .check:hover:not(:disabled) .ring {
    stroke: var(--color-accent);
  }
  .check:hover:not(:disabled) .tick {
    stroke-dashoffset: 0;
    opacity: 0.45;
    stroke: var(--color-accent);
  }

  /* Erledigt */
  .check[aria-checked="true"] .fill {
    transform: scale(1);
    opacity: 1;
  }
  .check[aria-checked="true"] .ring {
    stroke: var(--color-accent-fill);
  }
  .check[aria-checked="true"] .tick,
  .check[aria-checked="true"]:hover .tick {
    stroke: var(--color-on-accent);
    stroke-dashoffset: 0;
    opacity: 1;
    transition-delay: 40ms, 0ms;
  }
  .check:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
</style>
