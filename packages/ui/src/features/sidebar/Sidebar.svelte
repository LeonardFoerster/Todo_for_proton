<script lang="ts">
  import type { SyncStatusDto } from "@tasks/core-client";
  import logo from "../../assets/logo.png";
  import Button from "../../components/Button.svelte";
  import IconButton from "../../components/IconButton.svelte";
  import { Hash, PanelLeft, Plus } from "../../icons";
  import { formatCount, type ListsState } from "../../lib/lists.svelte";
  import { routeHash, type Route } from "../../lib/router.svelte";
  import { SMART_VIEWS } from "../../lib/views";
  import NewListInput from "./NewListInput.svelte";
  import SidebarItem from "./SidebarItem.svelte";
  import SyncFooter from "./SyncFooter.svelte";

  interface Props {
    route: Route;
    lists: ListsState;
    collapsed: boolean;
    status: SyncStatusDto;
    account: string | null;
    onnewtask: () => void;
    oncreatelist: (name: string) => Promise<void>;
    ontogglecollapse: () => void;
    onnavigate?: () => void;
    onlogin?: () => void;
    onsync?: () => void;
  }

  let {
    route,
    lists,
    collapsed,
    status,
    account,
    onnewtask,
    oncreatelist,
    ontogglecollapse,
    onnavigate,
    onlogin,
    onsync,
  }: Props = $props();

  let addingList = $state(false);

  function countFor(kind: string): string | undefined {
    if (kind === "inbox") return formatCount(lists.counts.inbox);
    if (kind === "today") return formatCount(lists.counts.today);
    return undefined;
  }
</script>

<nav class="sidebar" class:collapsed aria-label="Navigation">
  <div class="brand">
    {#if !collapsed}
      <img class="mark" src={logo} alt="" width="22" height="22" />
      <span class="name">Tasks</span>
    {/if}
    <IconButton
      label={collapsed ? "Seitenleiste ausklappen" : "Seitenleiste einklappen"}
      icon={PanelLeft}
      size={28}
      tooltipSide="right"
      onclick={ontogglecollapse}
    />
  </div>

  <div class="primary">
    {#if collapsed}
      <button type="button" class="new-compact" aria-label="Neue Task" data-tooltip="Neue Task   N" data-tooltip-side="right" onclick={onnewtask}>
        <Plus size={20} strokeWidth={2} aria-hidden="true" />
      </button>
    {:else}
      <Button variant="primary" size="lg" block icon={Plus} onclick={onnewtask} title="Neue Task (N)">Neue Task</Button>
    {/if}
  </div>

  <div class="scroll">
    <ul class="group">
      {#each SMART_VIEWS as view (view.kind)}
        <li>
          <SidebarItem
            href={routeHash({ view: view.kind })}
            icon={view.icon}
            label={view.label}
            count={countFor(view.kind)}
            active={route.view === view.kind}
            {collapsed}
            {onnavigate}
          />
        </li>
      {/each}
    </ul>

    <div class="section">
      {#if !collapsed}
        <span class="section-label" id="lists-label">Listen</span>
        <IconButton label="Neue Liste" icon={Plus} size={28} onclick={() => (addingList = true)} />
      {:else}
        <span class="divider"></span>
      {/if}
    </div>
    <ul class="group" aria-labelledby={collapsed ? undefined : "lists-label"}>
      {#each lists.userLists as list (list.id)}
        <li>
          <SidebarItem
            href={routeHash({ view: "list", listId: list.id })}
            icon={Hash}
            label={list.name}
            monogram={[...list.name][0]}
            active={route.view === "list" && route.listId === list.id}
            {collapsed}
            {onnavigate}
          />
        </li>
      {/each}
      {#if addingList && !collapsed}
        <li>
          <NewListInput
            oncreate={async (name) => {
              await oncreatelist(name);
              addingList = false;
            }}
            oncancel={() => (addingList = false)}
          />
        </li>
      {:else if lists.loaded && lists.userLists.length === 0 && !collapsed}
        <li class="hint">Eigene Listen für Projekte oder Bereiche – mit + anlegen.</li>
      {/if}
    </ul>
  </div>

  <SyncFooter {status} {account} {collapsed} {onlogin} {onsync} />
</nav>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 52px;
    padding: 0 var(--space-2) 0 var(--space-4);
  }
  .collapsed .brand {
    justify-content: center;
    padding: 0;
  }
  .mark {
    display: block;
    width: 22px;
    height: 22px;
  }
  .name {
    flex: 1;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
    letter-spacing: -0.01em;
  }
  .primary {
    padding: var(--space-1) var(--space-3) var(--space-3);
  }
  .collapsed .primary {
    display: flex;
    justify-content: center;
    padding: var(--space-1) 0 var(--space-3);
  }
  .new-compact {
    display: grid;
    place-items: center;
    width: 40px;
    height: 36px;
    border: 0;
    border-radius: var(--radius-md);
    background: var(--color-accent-fill);
    color: var(--color-on-accent);
    cursor: pointer;
  }
  .new-compact:hover {
    background: var(--color-accent-fill-hover);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--space-2);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .section {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    margin-top: var(--space-4);
    padding: 0 var(--space-1) 0 10px;
  }
  .collapsed .section {
    justify-content: center;
    height: 16px;
    padding: 0;
  }
  .section-label {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .divider {
    width: 24px;
    height: 1px;
    background: var(--color-border);
  }
  .hint {
    padding: var(--space-1) 10px;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    line-height: 1.4;
  }
</style>
