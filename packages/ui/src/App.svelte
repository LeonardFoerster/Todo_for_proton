<script lang="ts">
  import type { CoreClient } from "@tasks/core-client";
  import { onMount } from "svelte";
  import IconButton from "./components/IconButton.svelte";
  import Toaster from "./components/Toaster.svelte";
  import { toasts } from "./components/toasts.svelte";
  import Sidebar from "./features/sidebar/Sidebar.svelte";
  import QuickAdd from "./features/quick-add/QuickAdd.svelte";
  import TaskDetail from "./features/task-detail/TaskDetail.svelte";
  import TaskList from "./features/task-list/TaskList.svelte";
  import { PanelLeft } from "./icons";
  import { setCoreClient } from "./lib/context";
  import { ListsState } from "./lib/lists.svelte";
  import { readFlag, writeFlag } from "./lib/preferences";
  import { routeHash, Router } from "./lib/router.svelte";
  import { SyncState } from "./lib/sync.svelte";
  import { todayPlus } from "./lib/format-date";
  import { routeListId, routeTitle } from "./lib/views";
  import { deleteWithUndo } from "./lib/task-actions";
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
  let selectedId = $state<string | null>(null);
  let quickAdd = $state<QuickAdd>();
  let taskList = $state<TaskList>();
  let reportedReady = false;

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
    })();
    return () => stops.forEach((stop) => stop());
  });

  // Gelöschte oder unbekannte Liste → Eingang.
  $effect(() => {
    if (route.view === "list" && lists.loaded && !lists.lists.some((l) => l.id === route.listId)) {
      router.navigate({ view: "inbox" }, { replace: true });
    }
  });

  /** Erste Liste steht: Startzeit melden und Sync anstoßen (einmal). */
  function listReady() {
    if (reportedReady) return;
    reportedReady = true;
    void client.ready();
  }

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
    quickAdd?.focus();
  }

  function closeDetail() {
    selectedId = null;
  }

  /** Über die Liste löschen (kennt die Nachbarn für „Rückgängig“), sonst direkt. */
  function deleteFromDetail(id: string) {
    const task = taskList?.items().find((t) => t.id === id);
    selectedId = null;
    if (task && taskList) void taskList.deleteTask(task);
    else void deleteWithUndo(client, id);
  }
</script>

<div class="app" class:collapsed class:drawer-open={drawerOpen} class:detail-open={selectedId !== null}>
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
      {#if ready && (route.view !== "list" || lists.lists.some((l) => l.id === route.listId))}
        {#key routeHash(route)}
          {#if route.view !== "completed"}
            <QuickAdd
              bind:this={quickAdd}
              listId={routeListId(route)}
              defaultDue={route.view === "today" ? todayPlus(0) : null}
              placeholder={route.view === "today" ? "Task für heute hinzufügen" : "Task hinzufügen – z. B. „Zahnarzt anrufen morgen“"}
            />
          {/if}
          <div class="list-slot">
            <TaskList bind:this={taskList} {route} {lists} bind:selectedId onready={listReady} />
          </div>
        {/key}
      {/if}
    </div>
  </main>

  {#if selectedId}
    <aside class="detail-slot">
      <TaskDetail taskId={selectedId} {lists} onclose={closeDetail} ondelete={deleteFromDetail} />
    </aside>
  {/if}
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
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .list-slot {
    flex: 1;
    min-height: 0;
  }

  /* Detail: ≥ 1100 px eigene Spalte, darunter schwebend */
  .detail-slot {
    min-width: 0;
    min-height: 0;
    margin: var(--space-2) var(--space-2) var(--space-2) 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .app.detail-open {
    grid-template-columns: var(--sidebar-width) minmax(360px, 1fr) clamp(360px, 30vw, 420px);
  }
  @media (max-width: 1099px) {
    .app.detail-open {
      grid-template-columns: var(--sidebar-width) minmax(0, 1fr);
    }
    .detail-slot {
      position: fixed;
      top: var(--space-2);
      right: var(--space-2);
      bottom: var(--space-2);
      z-index: 20;
      width: min(400px, calc(100vw - 2 * var(--space-2)));
      margin: 0;
      box-shadow: var(--shadow-popover);
      animation: slide-in var(--duration-base) var(--ease);
    }
  }
  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateX(16px);
    }
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
    .detail-slot {
      inset: 0;
      width: auto;
      border: 0;
      border-radius: 0;
    }
  }
</style>
