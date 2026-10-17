<script lang="ts">
  import type { CoreClient } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import IconButton from "./components/IconButton.svelte";
  import Toaster from "./components/Toaster.svelte";
  import { toasts } from "./components/toasts.svelte";
  import Sidebar from "./features/sidebar/Sidebar.svelte";
  import { PanelLeft } from "./icons";
  import { setCoreClient } from "./lib/context";
  import { ListsState } from "./lib/lists.svelte";
  import { readFlag, writeFlag } from "./lib/preferences";
  import { Router } from "./lib/router.svelte";
  import { SyncState } from "./lib/sync.svelte";
  import { routeTitle, SMART_VIEWS } from "./lib/views";
  import { followSystemTheme } from "./theme/theme";
  import "./tokens/tokens.css";

  interface Props {
    client: CoreClient;
  }

  let { client }: Props = $props();
  // svelte-ignore state_referenced_locally
  setCoreClient(client);

  const COLLAPSED_KEY = "tasks.sidebar-collapsed";
  // svelte-ignore state_referenced_locally
  const lists = new ListsState(client);
  // svelte-ignore state_referenced_locally
  const sync = new SyncState(client);
  const router = new Router();

  let collapsed = $state(readFlag(COLLAPSED_KEY));
  let drawerOpen = $state(false);
  let ready = $state(false);

  const route = $derived(router.route);
  const title = $derived(routeTitle(route, lists.lists));

  onMount(() => {
    const stops = [router.listen(), followSystemTheme()];
    void (async () => {
      try {
        await client.ensureInbox();
      } catch (e) {
        toasts.show({ message: `Daten konnten nicht geladen werden: ${(e as Error).message}`, tone: "danger", timeoutMs: 0 });
      }
      stops.push(lists.start(), sync.start());
      ready = true;
      await tick();
      requestAnimationFrame(() => requestAnimationFrame(() => void client.ready()));
    })();
    return () => stops.forEach((stop) => stop());
  });

  // Gelöschte oder unbekannte Liste → Eingang.
  $effect(() => {
    if (route.view === "list" && lists.loaded && !lists.lists.some((l) => l.id === route.listId)) {
      router.navigate({ view: "inbox" }, { replace: true });
    }
  });

  function toggleCollapse() {
    if (matchMedia("(max-width: 699px)").matches) {
      drawerOpen = !drawerOpen;
      return;
    }
    collapsed = !collapsed;
    writeFlag(COLLAPSED_KEY, collapsed);
  }

  async function createList(name: string) {
    try {
      const list = await client.createTaskList(name);
      // Erst laden, sonst hält der Fallback „unbekannte Liste“ die neue für gelöscht.
      await lists.reloadLists();
      router.navigate({ view: "list", listId: list.id });
      drawerOpen = false;
    } catch (e) {
      toasts.show({ message: `Liste nicht angelegt: ${(e as Error).message}`, tone: "danger" });
    }
  }

  function newTask() {
    drawerOpen = false;
    // Quick-Add folgt in P7-08.
  }
</script>

<div class="app" class:collapsed class:drawer-open={drawerOpen}>
  <aside class="sidebar-slot">
    <Sidebar
      {route}
      {lists}
      collapsed={collapsed && !drawerOpen}
      status={sync.status}
      account={sync.account}
      onnewtask={newTask}
      oncreatelist={createList}
      ontogglecollapse={toggleCollapse}
      onnavigate={() => (drawerOpen = false)}
      onsync={() => void client.sync.now()}
    />
  </aside>
  {#if drawerOpen}
    <button type="button" class="scrim" aria-label="Navigation schließen" onclick={() => (drawerOpen = false)}></button>
  {/if}

  <main class="pane" aria-labelledby="view-title">
    <header class="pane-header">
      <span class="menu">
        <IconButton label="Navigation öffnen" icon={PanelLeft} onclick={() => (drawerOpen = true)} />
      </span>
      <h1 id="view-title">{title}</h1>
    </header>
    <div class="pane-body">
      {#if ready}
        <div class="empty">
          <p>{SMART_VIEWS.find((v) => v.kind === route.view)?.empty ?? "Diese Liste ist leer."}</p>
        </div>
      {/if}
    </div>
  </main>
</div>
<Toaster />

<style>
  :global(html, body, #app) {
    height: 100%;
  }
  :global(body) {
    overflow: hidden;
    background: var(--color-bg-subtle);
  }

  .app {
    --sidebar-width: 240px;
    display: grid;
    grid-template-columns: var(--sidebar-width) minmax(0, 1fr);
    height: 100%;
    background: var(--color-bg-subtle);
    color: var(--color-text);
  }
  .app.collapsed {
    --sidebar-width: 56px;
  }
  .sidebar-slot {
    min-height: 0;
    overflow: hidden;
  }

  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    margin: var(--space-2) var(--space-2) var(--space-2) 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg);
    overflow: hidden;
  }
  .pane-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 56px;
    padding: 0 var(--space-4) 0 var(--space-6);
    flex: none;
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    letter-spacing: -0.01em;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .menu {
    display: none;
    margin-left: calc(-1 * var(--space-3));
  }
  .pane-body {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .empty {
    display: grid;
    place-items: center;
    height: 100%;
    padding: var(--space-8);
    color: var(--color-text-muted);
    text-align: center;
  }
  .scrim {
    display: none;
  }

  /* < 700 px: eine Spalte, Navigation als Schublade */
  @media (max-width: 699px) {
    .app,
    .app.collapsed {
      grid-template-columns: minmax(0, 1fr);
    }
    .sidebar-slot {
      position: fixed;
      inset: 0 auto 0 0;
      z-index: 40;
      width: 280px;
      background: var(--color-bg-subtle);
      box-shadow: var(--shadow-popover);
      transform: translateX(-105%);
      transition: transform var(--duration-base) var(--ease);
    }
    .drawer-open .sidebar-slot {
      transform: none;
    }
    .scrim {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 30;
      border: 0;
      background: rgb(0 0 0 / 0.35);
    }
    .pane {
      margin: 0;
      border: 0;
      border-radius: 0;
    }
    .pane-header {
      padding-left: var(--space-4);
    }
    .menu {
      display: block;
    }
  }
</style>
