<script lang="ts">
  /**
   * Wiederholung im Detail (N4-06): ein Button mit der Regel im Klartext, der ein Menü öffnet – Keine,
   * Vorlagen, „Benutzerdefiniert…“ (lazy) und bei einer Regel „Wiederholung beenden“. Gleiche Auswahl wie
   * der Chip im Fenster „Neue Task“.
   */
  import type { DueDto, RecurrenceDto } from "@tasks/core-client";
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import { Check, Circle, Repeat, Settings, X } from "../../icons";
  import { describeRule, PRESETS, presetOf } from "../../lib/recurrence";
  import type RecurrenceDialogType from "./RecurrenceDialog.svelte";

  interface Props {
    value: RecurrenceDto | null;
    due: DueDto | null;
    onchange: (rule: RecurrenceDto | null) => void;
    id?: string;
  }

  let { value, due, onchange, id }: Props = $props();
  let button = $state<HTMLButtonElement>();
  let menu = $state<{ x: number; y: number } | null>(null);
  let custom = $state(false);
  let RecurrenceDialog = $state<typeof RecurrenceDialogType | null>(null);

  function openMenu() {
    const r = button?.getBoundingClientRect();
    menu = { x: r?.left ?? 0, y: (r?.bottom ?? 0) + 4 };
  }

  function items(): MenuItem[] {
    const current = value ? presetOf(value, due) : null;
    return [
      ...PRESETS.map((p) => ({ id: p.id, label: p.label, icon: p.id === current ? Check : Circle, action: () => pick(p.rule()) })),
      { id: "custom", label: "Benutzerdefiniert…", icon: value && !current ? Check : Settings, action: () => void openCustom() },
      ...(value ? [{ id: "end", label: "Wiederholung beenden", icon: X, action: () => pick(null) }] : []),
    ];
  }

  async function openCustom() {
    RecurrenceDialog ??= (await import("./RecurrenceDialog.svelte")).default;
    custom = true;
  }

  function pick(rule: RecurrenceDto | null) {
    custom = false;
    onchange(rule);
    button?.focus();
  }
</script>

<button
  bind:this={button}
  {id}
  type="button"
  class="field"
  class:empty={!value}
  aria-haspopup="menu"
  aria-expanded={menu !== null}
  onclick={openMenu}
>
  <Repeat size={16} strokeWidth={1.75} aria-hidden="true" />
  <span>{value ? describeRule(value) : "Keine"}</span>
</button>

{#if menu}
  <Menu label="Wiederholung wählen" items={items()} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}
{#if custom && RecurrenceDialog}
  <RecurrenceDialog
    {value}
    {due}
    onpick={pick}
    onclose={() => {
      custom = false;
      button?.focus();
    }}
  />
{/if}

<style>
  .field {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    max-width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-3) 0 var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--ease);
  }
  .field span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .field:hover,
  .field[aria-expanded="true"] {
    border-color: var(--color-accent);
  }
  .field :global(svg) {
    flex: none;
    color: var(--color-text-muted);
  }
  .field.empty {
    color: var(--color-text-muted);
  }
</style>
