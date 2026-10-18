<script lang="ts">
  import { GripVertical, type IconComponent } from "../../icons";

  interface Props {
    href: string;
    icon: IconComponent;
    label: string;
    count?: string;
    active?: boolean;
    collapsed?: boolean;
    /** Eingeklappt statt Icon (unterscheidet Listen). */
    monogram?: string;
    onnavigate?: () => void;
    /** Benutzerliste: ID für Kürzel/Ziehen, Kontextmenü, Griff zum Umsortieren. */
    listId?: string;
    oncontextmenu?: (e: MouseEvent) => void;
    ondragstart?: (e: PointerEvent) => void;
    dragging?: boolean;
  }

  let {
    href,
    icon: Icon,
    label,
    count,
    active = false,
    collapsed = false,
    monogram,
    onnavigate,
    listId,
    oncontextmenu,
    ondragstart,
    dragging = false,
  }: Props = $props();
</script>

<a
  {href}
  class="item"
  class:active
  class:collapsed
  class:dragging
  data-list-id={listId}
  aria-current={active ? "page" : undefined}
  aria-label={collapsed ? label : undefined}
  data-tooltip={collapsed ? label : undefined}
  data-tooltip-side="right"
  onclick={onnavigate}
  {oncontextmenu}
>
  {#if ondragstart && !collapsed}
    <span
      class="grip"
      aria-hidden="true"
      onpointerdown={(e) => ondragstart(e)}
      onclick={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <GripVertical size={14} strokeWidth={1.75} />
    </span>
  {/if}
  {#if collapsed && monogram}
    <span class="monogram" aria-hidden="true">{monogram}</span>
  {:else}
    <span class="icon"><Icon size={18} strokeWidth={1.75} aria-hidden="true" /></span>
  {/if}
  {#if !collapsed}
    <span class="label">{label}</span>
    {#if count && count !== "0"}<span class="count">{count}</span>{/if}
  {:else if count && count !== "0"}
    <span class="dot" aria-hidden="true"></span>
  {/if}
</a>

<style>
  .item {
    position: relative; /* für den Zähler-Punkt im eingeklappten Zustand */
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 34px;
    padding: 0 var(--space-3) 0 10px;
    border-radius: var(--radius-md);
    color: var(--color-text);
    text-decoration: none;
    font-size: var(--text-md);
    transition: background-color var(--duration-fast) var(--ease);
  }
  .item:hover {
    background: var(--color-bg-hover);
  }
  .icon {
    display: grid;
    place-items: center;
    color: var(--color-text-muted);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .count {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }
  .active {
    background: var(--color-bg-selected);
    color: var(--color-accent-text);
    font-weight: var(--weight-medium);
  }
  .active:hover {
    background: var(--color-bg-selected);
  }
  .active .icon,
  .active .count {
    color: var(--color-accent-text);
  }
  .collapsed {
    justify-content: center;
    padding: 0;
    width: 40px;
    margin: 0 auto;
  }
  .monogram {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--color-bg-active);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    text-transform: uppercase;
  }
  .active .monogram {
    background: var(--color-accent-fill);
    color: var(--color-on-accent);
  }
  .grip {
    position: absolute;
    left: -2px;
    top: 50%;
    display: grid;
    place-items: center;
    width: 14px;
    height: 24px;
    color: var(--color-text-muted);
    opacity: 0;
    transform: translateY(-50%);
    cursor: grab;
    touch-action: none;
  }
  .item:hover .grip {
    opacity: 1;
  }
  .dragging {
    opacity: 0.6;
    background: var(--color-bg-selected);
  }
  .dot {
    position: absolute;
    top: 7px;
    right: 7px;
    width: 6px;
    height: 6px;
    border-radius: var(--radius-full);
    background: var(--color-accent);
  }
</style>
