<script lang="ts">
  import type { CoreClient } from "@tasks/core-client";
  import { onMount, untrack } from "svelte";
  import IconButton from "./components/IconButton.svelte";
  import Toaster from "./components/Toaster.svelte";
  import { toasts } from "./components/toasts.svelte";
  import Sidebar from "./features/sidebar/Sidebar.svelte";
  import QuickAdd from "./features/quick-add/QuickAdd.svelte";
  import TaskDetail from "./features/task-detail/TaskDetail.svelte";
  import SelectionBar from "./features/task-list/SelectionBar.svelte";
  import TaskList from "./features/task-list/TaskList.svelte";
  import ShortcutHelp from "./features/shell/ShortcutHelp.svelte";
  import { Keyboard, ListChecks, PanelLeft, Settings } from "./icons";
  import type SettingsDialogType from "./features/account/SettingsDialog.svelte";
  import { setCoreClient } from "./lib/context";
  import { ListsState } from "./lib/lists.svelte";
  import { readFlag, writeFlag } from "./lib/preferences";
  import { routeHash, Router, type SmartViewKind } from "./lib/router.svelte";
  import { ShortcutRegistry } from "./lib/shortcuts";
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
  /** Hervorgehobene Zeile (Tastatur/Klick). */
  let selectedId = $state<string | null>(null);
  /** Im Detail-Panel geöffnete Task; folgt der Auswahl, solange das Panel offen ist. */
  let openId = $state<string | null>(null);
  let showShortcuts = $state(false);
  let showSettings = $state(false);
  /** Einstellungen/Login werden erst bei Bedarf geladen (eigener Chunk). */
  let SettingsDialog = $state<typeof SettingsDialogType | null>(null);
  let quickAdd = $state<QuickAdd>();
  let taskList = $state<TaskList>();
  let reportedReady = false;

  const route = $derived(router.route);
  const title = $derived(routeTitle(route, lists.lists));
  /** Mehrfachauswahl der aktuellen Ansicht (gehört der TaskList, endet mit ihr). */
  const selection = $derived(taskList?.selection);

  // Auswahlmodus schließt das Detail-Panel.
  $effect(() => {
    if (selection?.active) untrack(() => (openId = null));
  });

  onMount(() => {
    const stops = [router.listen(), followSystemTheme(), shortcuts.attach(window), trackInputMode()];
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

  async function openSettings() {
    drawerOpen = false;
    SettingsDialog ??= (await import("./features/account/SettingsDialog.svelte")).default;
    showSettings = true;
  }

  function closeDetail() {
    openId = null;
    void taskList?.focusSelected();
  }

  /** Über die Liste löschen (kennt die Nachbarn für „Rückgängig“), sonst direkt. */
  function deleteFromDetail(id: string) {
    const task = taskList?.items().find((t) => t.id === id);
    openId = null;
    if (task && taskList) void taskList.deleteTask(task);
    else void deleteWithUndo(client, id);
  }

  // --- Tastatur (ui-guidelines §5) -------------------------------------------------------------

  /**
   * Letzte Eingabeart als `data-input` auf `<html>` (keyboard|pointer): Bei Tastaturbedienung zeigt
   * die fokussierte Zeile immer einen Ring – unabhängig von der `:focus-visible`-Heuristik der Engine.
   */
  function trackInputMode(): () => void {
    const root = document.documentElement;
    const keyboard = (e: KeyboardEvent) => {
      if (!["Control", "Alt", "Shift", "Meta"].includes(e.key)) root.dataset.input = "keyboard";
    };
    const pointer = () => (root.dataset.input = "pointer");
    addEventListener("keydown", keyboard, true);
    addEventListener("pointerdown", pointer, true);
    return () => {
      removeEventListener("keydown", keyboard, true);
      removeEventListener("pointerdown", pointer, true);
    };
  }

  const shortcuts = new ShortcutRegistry();
  const hasList = () => !!taskList && taskList.items().length > 0;
  const selected = () => taskList?.items().find((t) => t.id === selectedId);
  const selecting = () => !!selection?.active;

  /** Auswahl verschieben; ohne Auswahl die erste bzw. letzte Zeile. */
  function selectBy(delta: -1 | 1) {
    const items = taskList?.items() ?? [];
    if (!items.length) return;
    const current = items.findIndex((t) => t.id === selectedId);
    const next = current < 0 ? (delta > 0 ? 0 : items.length - 1) : Math.max(0, Math.min(items.length - 1, current + delta));
    selectedId = items[next].id;
    if (openId) openId = selectedId;
    taskList?.scrollToIndex(next);
    void taskList?.focusSelected();
  }

  function goTo(view: SmartViewKind) {
    router.navigate({ view });
    selectedId = openId = null;
  }

  function escape(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    if (showShortcuts) showShortcuts = false;
    else if (showSettings) showSettings = false;
    else if (target && target.matches("input, textarea, select")) target.blur();
    else if (openId) closeDetail();
    else if (drawerOpen) drawerOpen = false;
    else if (selecting()) taskList?.exitSelection();
    else selectedId = null;
  }

  shortcuts.setModal(() => showShortcuts || showSettings);
  shortcuts.register(
    { keys: ["n"], description: "Neue Task", group: "Allgemein", run: newTask },
    { keys: ["ctrl+n"], description: "Neue Task (auch beim Tippen)", group: "Allgemein", inInputs: true, run: newTask },
    {
      keys: ["?"],
      description: "Tastaturkürzel anzeigen",
      group: "Allgemein",
      inModal: true,
      run: () => (showShortcuts = !showShortcuts),
    },
    {
      keys: ["escape"],
      description: "Schließen / Auswahl aufheben",
      group: "Allgemein",
      inInputs: true,
      inModal: true,
      run: escape,
    },
    { keys: ["arrowdown", "j"], label: "↓ / J", description: "Nächste Task", group: "Liste", when: hasList, run: () => selectBy(1) },
    { keys: ["arrowup", "k"], label: "↑ / K", description: "Vorige Task", group: "Liste", when: hasList, run: () => selectBy(-1) },
    {
      keys: ["space", "e"],
      label: "Leertaste / E",
      description: "Erledigt umschalten",
      group: "Liste",
      when: () => !selecting() && !!selected(),
      run: () => taskList?.toggleTask(selected()!),
    },
    {
      keys: ["enter"],
      description: "Details öffnen",
      group: "Liste",
      when: () => !selecting() && !!selected(),
      run: () => (openId = selectedId),
    },
    {
      keys: ["delete"],
      description: "Löschen (mit Rückgängig)",
      group: "Liste",
      when: () => !selecting() && !!selected(),
      run: () => void taskList?.deleteTask(),
    },
    { keys: ["alt+arrowup"], description: "Nach oben verschieben", group: "Liste", when: () => !!selected(), run: () => taskList?.moveSelected(-1) },
    { keys: ["alt+arrowdown"], description: "Nach unten verschieben", group: "Liste", when: () => !!selected(), run: () => taskList?.moveSelected(1) },
    {
      keys: ["ctrl+a"],
      description: "Alle auswählen (sind es schon alle: keine)",
      group: "Auswahl",
      when: hasList,
      run: () => void taskList?.toggleAll(),
    },
    {
      keys: ["x", "space"],
      label: "X / Leertaste",
      description: "Task aus- oder abwählen",
      group: "Auswahl",
      when: () => selecting() && !!selected(),
      run: () => taskList?.toggleCursor(),
    },
    {
      keys: ["shift+arrowdown"],
      description: "Auswahl nach unten erweitern",
      group: "Auswahl",
      when: hasList,
      run: () => taskList?.extendSelection(1),
    },
    {
      keys: ["shift+arrowup"],
      description: "Auswahl nach oben erweitern",
      group: "Auswahl",
      when: hasList,
      run: () => taskList?.extendSelection(-1),
    },
    { keys: ["g i"], description: "Eingang", group: "Ansichten", run: () => goTo("inbox") },
    { keys: ["g t"], description: "Heute", group: "Ansichten", run: () => goTo("today") },
    { keys: ["g u"], description: "Geplant", group: "Ansichten", run: () => goTo("upcoming") },
  );
</script>

<div class="app" class:collapsed class:drawer-open={drawerOpen} class:detail-open={openId !== null}>
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
      onlogin={() => void openSettings()}
      onsync={() => void openSettings()}
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
      {#if selection?.active && taskList}
        <SelectionBar
          all={taskList.selectionAll()}
          count={selection.count}
          ontoggleall={() => void taskList?.toggleAll()}
          ondone={() => taskList?.exitSelection()}
        />
      {:else}
        <span class="header-actions">
          <IconButton
            label="Auswählen"
            shortcut="Strg+A"
            icon={ListChecks}
            disabled={!hasList()}
            onclick={() => taskList?.startSelection()}
          />
          <IconButton label="Tastaturkürzel" shortcut="?" icon={Keyboard} onclick={() => (showShortcuts = true)} />
          <IconButton label="Einstellungen" icon={Settings} onclick={() => void openSettings()} />
        </span>
      {/if}
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
            <TaskList bind:this={taskList} {route} {lists} bind:selectedId onopen={(id) => (openId = id)} onready={listReady} onnewtask={newTask} />
          </div>
        {/key}
      {/if}
    </div>
  </main>

  {#if openId}
    <aside class="detail-slot">
      <TaskDetail taskId={openId} {lists} onclose={closeDetail} ondelete={deleteFromDetail} />
    </aside>
  {/if}
</div>
<Toaster />
{#if showShortcuts}
  <ShortcutHelp shortcuts={shortcuts.list()} onclose={() => (showShortcuts = false)} />
{/if}
{#if showSettings && SettingsDialog}
  <SettingsDialog
    {sync}
    onclose={() => (showSettings = false)}
    onshortcuts={() => {
      showSettings = false;
      showShortcuts = true;
    }}
  />
{/if}

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
  .header-actions {
    display: flex;
    gap: var(--space-1);
    margin-left: auto;
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
