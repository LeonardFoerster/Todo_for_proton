<script lang="ts">
  /**
   * Liste wählen (P2-04): Button mit dem Namen der gewählten Liste, öffnet ein Menü mit allen Listen
   * (Eingang zuerst) und „Neue Liste…“ (legt an und wählt sie). Für Quick-Add und Detail-Panel.
   */
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { Check, ChevronDown, Hash, Inbox, Plus } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import type { ListsState } from "../../lib/lists.svelte";
  import { listName } from "../../lib/views";
  import ListNameDialog from "./ListNameDialog.svelte";

  interface Props {
    lists: ListsState;
    /** Gewählte Liste; `undefined` = Eingang. */
    value: string | undefined;
    onchange: (listId: string) => void;
    id?: string;
    /** Tooltip-Kürzel (z. B. „Alt+L“ im Quick-Add). */
    shortcut?: string;
  }

  let { lists, value, onchange, id, shortcut }: Props = $props();

  const client = useCoreClient();
  let button = $state<HTMLButtonElement>();
  let menu = $state<{ x: number; y: number } | null>(null);
  let creating = $state(false);

  const current = $derived(lists.lists.find((l) => (value ? l.id === value : l.isInbox)));
  const name = $derived(current ? listName(current) : "Eingang");

  /** Menü unter dem Button öffnen (auch per Kürzel von außen). */
  export function open() {
    const r = button?.getBoundingClientRect();
    menu = { x: r?.left ?? 0, y: (r?.bottom ?? 0) + 4 };
  }

  function items(): MenuItem[] {
    return [
      ...lists.lists.map((l) => ({
        id: l.id,
        label: listName(l),
        icon: l.id === current?.id ? Check : l.isInbox ? Inbox : Hash,
        action: () => onchange(l.id),
      })),
      { id: "__new", label: "Neue Liste…", icon: Plus, action: () => (creating = true) },
    ];
  }

  async function create(listName: string) {
    try {
      const list = await client.createTaskList(listName);
      await lists.reloadLists();
      onchange(list.id);
    } catch (e) {
      toasts.show({ message: `Liste nicht angelegt: ${(e as Error).message}`, tone: "danger" });
      throw e;
    }
  }
</script>

<button
  bind:this={button}
  {id}
  type="button"
  class="picker"
  aria-haspopup="menu"
  aria-expanded={menu !== null}
  aria-label="Liste: {name}"
  data-tooltip={shortcut ? `Liste wählen   ${shortcut}` : "Liste wählen"}
  onclick={open}
>
  {#if current?.isInbox ?? true}
    <Inbox size={14} strokeWidth={1.75} aria-hidden="true" />
  {:else}
    <Hash size={14} strokeWidth={1.75} aria-hidden="true" />
  {/if}
  <span class="name">{name}</span>
  <ChevronDown size={14} strokeWidth={1.75} aria-hidden="true" />
</button>

{#if menu}
  <Menu label="Liste wählen" items={items()} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}
{#if creating}
  <ListNameDialog title="Neue Liste" submitLabel="Anlegen" onsubmit={create} onclose={() => (creating = false)} />
{/if}

<style>
  .picker {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
    max-width: 180px;
    height: 26px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-bg);
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease),
      color var(--duration-fast) var(--ease);
  }
  .picker:hover,
  .picker[aria-expanded="true"] {
    background: var(--color-bg-hover);
    color: var(--color-text);
  }
  .picker:focus-visible {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: 1px;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
