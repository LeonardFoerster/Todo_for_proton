<script lang="ts">
  import type { CoreClient, DueDto } from "@tasks/core-client";
  import { onMount, tick, untrack } from "svelte";
  import IconButton from "./components/IconButton.svelte";
  import Toaster from "./components/Toaster.svelte";
  import Tooltips from "./components/Tooltips.svelte";
  import { toasts } from "./components/toasts.svelte";
  import Sidebar from "./features/sidebar/Sidebar.svelte";
  import NewTaskDialog from "./features/new-task/NewTaskDialog.svelte";
  import type TaskDetailType from "./features/task-detail/TaskDetail.svelte";
  import SelectionBar from "./features/task-list/SelectionBar.svelte";
  import TaskList from "./features/task-list/TaskList.svelte";
  import ShortcutHelp from "./features/shell/ShortcutHelp.svelte";
  import {
    ArrowDown,
    ArrowUp,
    CalendarDays,
    CircleCheck,
    Ellipsis,
    FolderInput,
    List,
    ListChecks,
    PanelLeft,
    Plus,
    Pencil,
    Hash,
    Inbox as InboxIcon,
    Merge,
    SlidersHorizontal,
    SquareKanban,
    Trash,
    Undo,
    X,
  } from "./icons";
  import DuePrompt from "./features/due/DuePrompt.svelte";
  import { applyDue, dueChoices, type DueTarget } from "./features/due/due-actions";
  import Board from "./features/board/Board.svelte";
  import Menu, { type MenuItem } from "./components/Menu.svelte";
  import DeleteListDialog from "./features/lists/DeleteListDialog.svelte";
  import ListNameDialog from "./features/lists/ListNameDialog.svelte";
  import type { TagDto, TaskListDto } from "@tasks/core-client";
  import Button from "./components/Button.svelte";
  import Dialog from "./components/Dialog.svelte";
  import type SettingsDialogType from "./features/account/SettingsDialog.svelte";
  import type CommandPaletteType from "./features/palette/CommandPalette.svelte";
  import type { PaletteCommand, RecentTask } from "./features/palette/CommandPalette.svelte";
  import { setCoreClient } from "./lib/context";
  import { ListsState } from "./lib/lists.svelte";
  import { readChoice, readFlag, writeChoice, writeFlag } from "./lib/preferences";
  import { routeHash, Router, type SmartViewKind } from "./lib/router.svelte";
  import { formatKeys, ShortcutRegistry, type Shortcut } from "./lib/shortcuts";
  import { AppMode, type Mode } from "./lib/app-mode.svelte";
  import { exportAll, importFile } from "./lib/data-actions";
  import { density } from "./lib/density.svelte";
  import PurgeDialog from "./features/task-list/PurgeDialog.svelte";
  import type * as NotesChunk from "./features/notes/index";
  import { SyncState } from "./lib/sync.svelte";
  import { todayPlus } from "./lib/format-date";
  import { viewSubtitle } from "./lib/subtitle";
  import { routeListId, routeTitle } from "./lib/views";
  import {
    isDefault,
    optionsKey,
    PRIORITY_LABELS,
    readOptions,
    sortLabel,
    writeOptions,
    type ViewOptions,
  } from "./lib/view-options";
  import ViewOptionsPopover from "./features/view-options/ViewOptionsPopover.svelte";
  import { completedBefore, trashManyWithUndo } from "./lib/task-actions";
  import CleanupCompletedDialog from "./features/task-list/CleanupCompletedDialog.svelte";
  import { addDays } from "./lib/local-day";
  import { followSystemTheme } from "./theme/theme";
  import "./tokens/tokens.css";

  interface Props {
    client: CoreClient;
  }

  let { client }: Props = $props();
  // svelte-ignore state_referenced_locally
  setCoreClient(client);

  const COLLAPSED_KEY = "tasks.sidebar-collapsed";
  const LAYOUT_KEY = "tasks.layout";
  // svelte-ignore state_referenced_locally
  const lists = new ListsState(client);
  // svelte-ignore state_referenced_locally
  const sync = new SyncState(client);
  const router = new Router();
  /** Modus Tasks/Notes (N5), pro Gerät gemerkt. */
  const appMode = new AppMode();

  let collapsed = $state(readFlag(COLLAPSED_KEY));
  /** Darstellung Liste oder Kanban-Board (P2-05), pro Gerät gemerkt. */
  let layout = $state(readChoice(LAYOUT_KEY, ["list", "board"] as const, "list"));
  let drawerOpen = $state(false);
  let ready = $state(false);
  /** Hervorgehobene Zeile (Tastatur/Klick). */
  let selectedId = $state<string | null>(null);
  /** Im Detail-Panel geöffnete Task; folgt der Auswahl, solange das Panel offen ist. */
  let openId = $state<string | null>(null);
  /** Detail-Panel erst beim ersten Öffnen laden (hält das Start-Bundle klein). */
  let TaskDetail = $state<typeof TaskDetailType | null>(null);
  $effect(() => {
    if (openId && !TaskDetail) {
      void import("./features/task-detail/TaskDetail.svelte").then((m) => (TaskDetail = m.default));
    }
  });
  let showShortcuts = $state(false);
  /** Listen verwalten (P2-03): Menü, Inline-Umbenennen in der Seitenleiste, Dialoge. */
  let listMenu = $state<{ list: TaskListDto; x: number; y: number } | null>(null);
  let renamingId = $state<string | null>(null);
  let renameDialog = $state<TaskListDto | null>(null);
  let deleteTarget = $state<TaskListDto | null>(null);
  /** Menü „Fälligkeit“ bzw. Eingabe „Datum wählen…“ für diese Tasks (P4-04). */
  let dueMenu = $state<{ targets: DueTarget[]; x: number; y: number } | null>(null);
  let duePrompt = $state<DueTarget[] | null>(null);
  let showSettings = $state(false);
  /** Einstellungen/Login werden erst bei Bedarf geladen (eigener Chunk). */
  let SettingsDialog = $state<typeof SettingsDialogType | null>(null);
  /** Fenster „Neue Task“ (ersetzt die Eingabezeile über der Liste). */
  let showNewTask = $state(false);
  /** Vorbelegung der Titelzeile von „Neue Task“ (aus der Palette ohne Treffer). */
  let newTaskText = $state("");
  /** Befehlspalette (P4-05): erst beim ersten Öffnen geladen. */
  let CommandPalette = $state<typeof CommandPaletteType | null>(null);
  let showPalette = $state(false);
  let paletteCommands = $state.raw<PaletteCommand[]>([]);
  let showNewList = $state(false);
  /** „Erledigte aufräumen“ (P4-06): Menü der Ansicht und Bestätigung. */
  let completedMenu = $state<{ x: number; y: number } | null>(null);
  /** Papierkorb (ADR-031): Menü der Kopfzeile und Bestätigung „Papierkorb leeren“. */
  let trashMenu = $state<{ x: number; y: number } | null>(null);
  let emptyTrashConfirm = $state(false);

  async function emptyTrash() {
    try {
      const n = await client.emptyTaskTrash();
      toasts.show({ message: n === 1 ? "1 Task endgültig gelöscht" : `${n} Tasks endgültig gelöscht` });
    } catch (e) {
      toasts.show({ message: `Nicht gelöscht: ${(e as Error).message}`, tone: "danger" });
    }
  }
  /** Menü der Kopfzeile im schmalen Fenster (P5-09). */
  let viewMenu = $state<{ x: number; y: number; anchor: Element } | null>(null);
  let cleanup = $state<{ ids: string[]; cutoff: Date } | null>(null);
  /** Zuletzt geöffnete Tasks dieser Sitzung (neueste zuerst, nur im Speicher). */
  let recentTasks = $state.raw<RecentTask[]>([]);
  /** Notes-Chunk (lazy, `notes.md` §5.2): einmal geladen, danach bleibt alles gemountet. */
  let notesChunk = $state<typeof NotesChunk | null>(null);
  let notes = $state<InstanceType<typeof NotesChunk.NotesState> | null>(null);
  let notesMain = $state<{ flush: () => Promise<void> }>();
  let notesSidebar = $state<{ focusTree: () => void }>();
  let newTaskDialog = $state<NewTaskDialog>();
  let taskList = $state<TaskList>();
  let board = $state<Board>();
  let reportedReady = false;

  const route = $derived(router.route);
  /** Sortierung/Filter der Ansicht (N10-01), gemerkt pro Gerät. */
  // Beschreibbar abgeleitet: neue Ansicht → gemerkte Werte; `setOptions` überschreibt bis zum Wechsel.
  let viewOptions = $derived<ViewOptions>(readOptions(route));
  let optionsOpen = $state(false);
  let optionsButton = $state<HTMLButtonElement>();

  function setOptions(next: ViewOptions) {
    viewOptions = next;
    writeOptions(route, next);
  }

  /** Aktive Abweichungen als Chips unter der Kopfzeile. */
  const optionChips = $derived.by(() => {
    const chips: { label: string; clear: () => void }[] = [];
    if (viewOptions.sort !== "view") {
      chips.push({ label: `Sortiert: ${sortLabel(viewOptions.sort, { view: "all" })}`, clear: () => setOptions({ ...viewOptions, sort: "view" }) });
    }
    if (viewOptions.priority) {
      chips.push({ label: `Priorität: ${PRIORITY_LABELS[viewOptions.priority]}`, clear: () => setOptions({ ...viewOptions, priority: null }) });
    }
    const tag = viewOptions.tag && route.view !== "tag" ? lists.tags.find((t) => t.id === viewOptions.tag) : undefined;
    if (tag) chips.push({ label: `#${tag.name}`, clear: () => setOptions({ ...viewOptions, tag: null }) });
    return chips;
  });
  const title = $derived(routeTitle(route, lists.lists, lists.tags));
  /** Das Board zeigt alle Listen; umschaltbar in Eingang, Listen und „Alle Tasks“. */
  const boardCapable = $derived(route.view === "inbox" || route.view === "list" || route.view === "all");
  const showBoard = $derived(boardCapable && layout === "board");
  /** Unterzeile im Kopf (P5-05): Datum in „Heute“, Zahlen nur, wo sie ohne neuen Core-Aufruf bekannt sind; im Board (alle Listen) keine. */
  const subtitle = $derived(showBoard ? "" : viewSubtitle(route, lists.counts));

  function setLayout(next: "list" | "board") {
    if (next === layout) return;
    layout = next;
    writeChoice(LAYOUT_KEY, next);
  }
  /** Mehrfachauswahl der aktuellen Ansicht (gehört der TaskList, endet mit ihr). */
  const selection = $derived(taskList?.selection);

  // Auswahlmodus schließt das Detail-Panel.
  $effect(() => {
    if (selection?.active) untrack(() => (openId = null));
  });

  /** Lädt den Notes-Chunk und startet seinen Zustand (einmal). */
  async function loadNotes() {
    if (notes) return;
    const chunk = await import("./features/notes/index");
    if (notes) return;
    notesChunk = chunk;
    const state = new chunk.NotesState(client);
    // Verknüpfte Tasks aus der Statuszeile der Notiz (ADR-036): in den Modus „Tasks“, Task öffnen.
    state.openTask = (id) => void openTaskInTasks(id);
    notes = state;
    notesStop = state.start();
    notesShortcutsStop = shortcuts.register(...notesShortcuts(state));
  }
  let notesStop: (() => void) | null = null;
  let notesShortcutsStop: (() => void) | null = null;

  /** Kürzel des Modus „Notes", die zur App gehören (Neue Notiz); der Baum registriert seine. */
  function notesShortcuts(state: InstanceType<typeof NotesChunk.NotesState>): Shortcut[] {
    const create = () => void state.createNote().catch((e: Error) => toasts.show({ message: `Notiz nicht angelegt: ${e.message}`, tone: "danger" }));
    return inScope("notes", [
      { keys: ["n"], description: "Neue Notiz", group: "Notizen", palette: "Neue Notiz", run: create },
      { keys: ["ctrl+n"], description: "Neue Notiz (auch beim Tippen)", group: "Notizen", inInputs: true, run: create },
      {
        keys: ["ctrl+shift+f"],
        description: "Zen-Modus an/aus",
        group: "Notizen",
        palette: "Zen-Modus an/aus",
        inInputs: true,
        run: () => state.setZen(!state.zen),
      },
    ]);
  }

  function inScope(scope: Mode, list: Shortcut[]): Shortcut[] {
    return list.map((s) => ({ ...s, scope }));
  }

  /** Modus wechseln (Klick auf den Namen, Strg+1/2, Palette): Eingaben sichern, sofort hochladen. */
  async function switchMode(next: Mode) {
    if (next === appMode.mode) return;
    drawerOpen = false;
    if (appMode.mode === "notes") await notesMain?.flush();
    if (next === "notes") await loadNotes();
    appMode.set(next);
    void client.sync.flush().catch(() => undefined);
    await tick();
    if (next === "notes") {
      if (notes?.openId && !notes.showTrash) notes.focusEditor();
      else notesSidebar?.focusTree();
    } else void taskList?.focusSelected();
  }

  onMount(() => {
    const stops = [router.listen(), followSystemTheme(), shortcuts.attach(window), trackInputMode()];
    stops.push(() => {
      notesStop?.();
      notesShortcutsStop?.();
    });
    // Vor dem Schließen die offene Notiz speichern (die Shell wartet darauf, `notes.md` §6.2).
    const offClose = client.onCloseRequested?.(async () => {
      await notesMain?.flush();
    });
    if (offClose) stops.push(offClose);
    // Shell (ADR-033): Klick auf eine Erinnerung öffnet die Task, „Neue Task“ im Tray das Fenster dafür.
    const offShell = client.background?.onShellAction((action) => {
      if (action.kind === "newTask") {
        newTask();
        return;
      }
      void (async () => {
        if (appMode.mode !== "tasks") await switchMode("tasks");
        openFromPalette(action.taskId);
      })();
    });
    if (offShell) stops.push(offShell);
    if (appMode.mode === "notes") void loadNotes();
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

  // Gelöschte oder unbekannte Liste bzw. Tag → Eingang.
  $effect(() => {
    if (route.view === "list" && lists.loaded && !lists.lists.some((l) => l.id === route.listId)) {
      router.navigate({ view: "inbox" }, { replace: true });
    }
    if (route.view === "tag" && lists.tagsLoaded && !lists.tags.some((t) => t.id === route.tagId)) {
      router.navigate({ view: "inbox" }, { replace: true });
    }
  });

  /** Die Task-Liste rendert auch versteckt; als Start zählt die sichtbare Ansicht des Modus. */
  let taskListReady = $state(false);
  function listReady() {
    taskListReady = true;
  }

  // Erste Liste bzw. Notizbaum steht: Startzeit melden und Sync anstoßen (einmal).
  $effect(() => {
    if (reportedReady) return;
    if (appMode.mode === "tasks" ? !taskListReady : !notes?.loaded) return;
    reportedReady = true;
    void client.ready();
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
    newTaskText = "";
    drawerOpen = false;
    showNewTask = true;
  }

  // --- Erledigte aufräumen (P4-06) ------------------------------------------------------------

  const CLEANUP_DAYS = 30;

  async function startCleanup() {
    const now = new Date();
    const cutoff = addDays(new Date(now.getFullYear(), now.getMonth(), now.getDate()), -CLEANUP_DAYS);
    try {
      const old = await completedBefore(client, cutoff);
      if (old.length === 0) toasts.show({ message: `Keine erledigten Tasks älter als ${CLEANUP_DAYS} Tage.` });
      else cleanup = { ids: old.map((t) => t.id), cutoff };
    } catch (e) {
      toasts.show({ message: `Nicht geladen: ${(e as Error).message}`, tone: "danger" });
    }
  }

  function confirmCleanup() {
    const ids = cleanup?.ids ?? [];
    cleanup = null;
    if (ids.length > 0) void trashManyWithUndo(client, ids);
  }

  // --- Befehlspalette (P4-05) -----------------------------------------------------------------

  /** Befehle = Registry-Einträge mit `palette`, die gerade gelten (ausgeführt ohne Tastaturereignis). */
  function commandsNow(): PaletteCommand[] {
    return shortcuts
      .list()
      .filter((s) => s.palette && shortcuts.inScope(s) && (!s.when || s.when()))
      .map((s) => ({
        id: s.palette!,
        label: s.palette!,
        shortcut: s.keys.length > 0 ? (s.label ?? formatKeys(s.keys[0])) : undefined,
        run: () => s.run(new KeyboardEvent("keydown")),
      }));
  }

  async function openPalette() {
    drawerOpen = false;
    CommandPalette ??= (await import("./features/palette/CommandPalette.svelte")).default;
    // Tags als Sprungziele (ADR-035): „#uni“ in der Palette findet „Gehe zu #uni“.
    paletteCommands = [
      ...commandsNow(),
      ...lists.tags.map((t) => ({
        id: `tag:${t.id}`,
        label: `Gehe zu #${t.name}`,
        run: () => router.navigate({ view: "tag", tagId: t.id }),
      })),
    ];
    showPalette = true;
  }

  /** Notiz aus der Suche: in den Modus „Notes", öffnen, Cursor auf den Treffer (`notes.md` §7). */
  async function openNote(id: string, hit: { offset: number; length: number } | null) {
    await switchMode("notes");
    if (!notes) return;
    const note = await client.notes.get(id).catch(() => null);
    if (!note) return;
    await notes.reveal(note.folderId);
    notes.select = hit ? { id, start: hit.offset, end: hit.offset + hit.length } : null;
    notes.open(id);
    notes.cursor = `n:${id}`;
    notes.focusEditor();
  }

  function openFromPalette(id: string) {
    selectedId = id;
    openId = id;
  }

  /** Aus dem Modus „Notes“: zurück zu den Tasks und die Task öffnen. */
  async function openTaskInTasks(id: string) {
    await switchMode("tasks");
    selectedId = id;
    openId = id;
  }

  // Geöffnete Task merken (Palette ohne Text zeigt sie); Titel aus dem Core.
  $effect(() => {
    const id = openId;
    if (!id) return;
    void client.getTask(id).then(
      (t) => (recentTasks = [{ id: t.id, title: t.title }, ...recentTasks.filter((r) => r.id !== t.id)].slice(0, 5)),
      () => undefined,
    );
  });

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
    else void trashManyWithUndo(client, [id]);
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
  const hasSelection = () => selecting() && (selection?.count ?? 0) > 0;

  /** „Verschieben nach…“ unter dem Button der Aktionsleiste öffnen (auch per `M`). */
  function openMoveMenu() {
    const button = document.querySelector<HTMLElement>("[data-move-button]");
    const rect = button?.getBoundingClientRect();
    taskList?.openMoveMenu(rect?.left ?? 0, (rect?.bottom ?? 0) + 4);
  }

  // --- Fälligkeit (P4-04) ---------------------------------------------------------------------

  /** Worauf T/Umschalt+T/D wirken: Board-Cursor, sonst die Auswahl, sonst die fokussierte Zeile. */
  function dueTargets(): DueTarget[] {
    if (showBoard) {
      const card = board?.cursorTask();
      return card ? [card] : [];
    }
    if (hasSelection()) return taskList?.selectedTasks() ?? [];
    const task = selecting() ? undefined : selected();
    return task ? [task] : [];
  }

  function setDue(targets: DueTarget[], due: DueDto | null) {
    if (targets.length > 0) void applyDue(client, targets, due);
  }

  /** Menü „Fälligkeit“ unter dem Button der Auswahlleiste bzw. an der fokussierten Zeile/Karte. */
  function openDueMenu(targets = dueTargets(), at?: { x: number; y: number }) {
    if (targets.length === 0) return;
    const anchor = hasSelection() ? document.querySelector("[data-due-button]") : document.activeElement;
    const rect = anchor?.getBoundingClientRect();
    dueMenu = { targets, x: at?.x ?? rect?.left ?? 0, y: at?.y ?? (rect?.bottom ?? 0) + 4 };
  }

  function dueMenuItems(targets: DueTarget[]): MenuItem[] {
    const shortcut = { today: "T", tomorrow: "Umschalt+T" } as Record<string, string | undefined>;
    const items: MenuItem[] = dueChoices()
      .filter((c) => c.id !== "none")
      .map((c) => ({ id: c.id, label: c.label, icon: CalendarDays, shortcut: shortcut[c.id], action: () => setDue(targets, c.due) }));
    items.push({ label: "Datum wählen…", icon: CalendarDays, action: () => (duePrompt = targets) });
    if (targets.some((t) => t.due)) items.push({ label: "Fälligkeit entfernen", icon: X, action: () => setDue(targets, null) });
    return items;
  }

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

  // --- Listen verwalten (P2-03) ----------------------------------------------------------------

  /** Seitenleiste sichtbar und ausgeklappt → inline umbenennen, sonst per Dialog. */
  function startRename(list: TaskListDto) {
    const narrow = typeof matchMedia === "function" && matchMedia("(max-width: 699px)").matches;
    if (!collapsed && (!narrow || drawerOpen)) renamingId = list.id;
    else renameDialog = list;
  }

  async function renameList(list: TaskListDto, name: string) {
    try {
      await lists.rename(list.id, name);
      renamingId = null;
    } catch (e) {
      renamingId = null;
      toasts.show({ message: `Nicht umbenannt: ${(e as Error).message}`, tone: "danger" });
      throw e;
    }
  }

  /** Benutzerliste an Einfügeposition `to` schieben; Fokus bleibt auf ihrem Eintrag. */
  async function moveList(id: string, to: number) {
    const hadFocus = (document.activeElement as HTMLElement | null)?.dataset.listId === id;
    const done = lists.move(id, to).catch((e: Error) => {
      toasts.show({ message: `Liste nicht verschoben: ${e.message}`, tone: "danger" });
    });
    if (hadFocus) {
      await tick();
      document.querySelector<HTMLElement>(`[data-list-id="${CSS.escape(id)}"]`)?.focus();
    }
    await done;
  }

  async function confirmDelete() {
    const list = deleteTarget;
    deleteTarget = null;
    if (!list) return;
    try {
      const moved = await lists.remove(list.id);
      const rest = moved === 0 ? "" : ` · ${moved === 1 ? "1 Task" : `${moved} Tasks`} im Eingang`;
      toasts.show({ message: `Liste „${list.name}“ gelöscht${rest}` });
    } catch (e) {
      toasts.show({ message: `Nicht gelöscht: ${(e as Error).message}`, tone: "danger" });
    }
  }

  function listMenuItems(list: TaskListDto): MenuItem[] {
    const user = lists.userLists;
    const i = user.findIndex((l) => l.id === list.id);
    return [
      { label: "Umbenennen", icon: Pencil, action: () => startRename(list) },
      { label: "Nach oben", icon: ArrowUp, shortcut: "Alt+↑", disabled: i <= 0, action: () => void moveList(list.id, i - 1) },
      {
        label: "Nach unten",
        icon: ArrowDown,
        shortcut: "Alt+↓",
        disabled: i < 0 || i >= user.length - 1,
        action: () => void moveList(list.id, i + 2),
      },
      {
        label: "Zusammenführen mit…",
        icon: Merge,
        disabled: lists.lists.length < 2,
        action: () => (mergeMenu = { list, x: listMenu?.x ?? 0, y: listMenu?.y ?? 0 }),
      },
      { label: "Löschen…", icon: Trash, danger: true, action: () => (deleteTarget = list) },
    ];
  }

  // --- Listen zusammenführen (N10-03, ADR-020) ---
  let mergeMenu = $state<{ list: TaskListDto; x: number; y: number } | null>(null);

  function mergeTargets(source: TaskListDto): MenuItem[] {
    return lists.lists
      .filter((l) => l.id !== source.id)
      .map((l) => ({
        id: l.id,
        label: l.isInbox ? "Eingang" : l.name,
        icon: l.isInbox ? InboxIcon : Hash,
        action: () => void mergeLists(source, l),
      }));
  }

  async function mergeLists(source: TaskListDto, target: TaskListDto) {
    try {
      const moved = await lists.merge(source.id, target.id);
      if (route.view === "list" && route.listId === source.id) {
        router.navigate(target.isInbox ? { view: "inbox" } : { view: "list", listId: target.id }, { replace: true });
      }
      const name = target.isInbox ? "Eingang" : target.name;
      const rest = moved === 0 ? "" : ` · ${moved === 1 ? "1 Task" : `${moved} Tasks`} verschoben`;
      toasts.show({ message: `„${source.name}“ in „${name}“ zusammengeführt${rest}` });
    } catch (e) {
      toasts.show({ message: `Nicht zusammengeführt: ${(e as Error).message}`, tone: "danger" });
    }
  }

  /** Gleichnamige Listen (z. B. nach dem ersten Login auf einem zweiten Gerät): einmal je Sitzung anbieten. */
  const hinted: string[] = [];
  $effect(() => {
    if (!lists.loaded) return;
    for (const group of lists.duplicateGroups()) {
      const key = group.map((l) => l.id).join("|");
      if (hinted.includes(key)) continue;
      hinted.push(key);
      const [first, ...rest] = group;
      toasts.show({
        message: `${group.length} Listen heißen „${first.name}“`,
        actionLabel: "Zusammenführen",
        timeoutMs: 10_000,
        onAction: () =>
          void (async () => {
            for (const other of rest) await mergeLists(other, first);
          })(),
      });
    }
  });

  /** Fokussierter Benutzerlisten-Eintrag in der Seitenleiste (für Kürzel). */
  function focusedList(): TaskListDto | undefined {
    const id = (document.activeElement as HTMLElement | null)?.dataset?.listId;
    return id ? lists.userLists.find((l) => l.id === id) : undefined;
  }

  function openListMenuAt(list: TaskListDto, el: Element) {
    const r = el.getBoundingClientRect();
    listMenu = { list, x: r.left, y: r.bottom + 4 };
  }

  const currentList = $derived(route.view === "list" ? lists.userLists.find((l) => l.id === route.listId) : undefined);
  const currentTag = $derived(route.view === "tag" ? lists.tags.find((t) => t.id === route.tagId) : undefined);
  let tagMenu = $state<{ x: number; y: number } | null>(null);
  let tagRename = $state<TagDto | null>(null);
  let tagDelete = $state<TagDto | null>(null);

  function tagMenuItems(tag: TagDto): MenuItem[] {
    return [
      { label: "Umbenennen", icon: Pencil, action: () => (tagRename = tag) },
      { label: "Löschen…", icon: Trash, danger: true, action: () => (tagDelete = tag) },
    ];
  }

  async function renameTag(tag: TagDto, name: string) {
    try {
      await client.renameTag(tag.id, name);
      await lists.reloadTags();
    } catch (e) {
      toasts.show({ message: `Nicht umbenannt: ${(e as Error).message}`, tone: "danger" });
      throw e;
    }
  }

  async function confirmTagDelete() {
    const tag = tagDelete;
    tagDelete = null;
    if (!tag) return;
    try {
      const n = await client.deleteTag(tag.id);
      await lists.reloadTags();
      const rest = n === 0 ? "" : ` · von ${n === 1 ? "1 Task" : `${n} Tasks`} entfernt`;
      toasts.show({ message: `Tag „#${tag.name}“ gelöscht${rest}` });
    } catch (e) {
      toasts.show({ message: `Nicht gelöscht: ${(e as Error).message}`, tone: "danger" });
    }
  }

  /** Einträge des Kopfzeilen-Menüs im schmalen Fenster: dasselbe wie die Icons der breiten Kopfzeile. */
  function viewMenuItems(at: { x: number; y: number; anchor: Element }): MenuItem[] {
    const items: MenuItem[] = [];
    if (boardCapable) {
      items.push(
        layout === "list"
          ? { label: "Als Kanban anzeigen", icon: SquareKanban, shortcut: "V", action: () => setLayout("board") }
          : { label: "Als Liste anzeigen", icon: List, shortcut: "V", action: () => setLayout("list") },
      );
    }
    items.push({
      label: "Auswählen",
      icon: ListChecks,
      disabled: showBoard || !hasList(),
      title: showBoard ? "Nur in der Listenansicht" : undefined,
      action: () => taskList?.startSelection(),
    });
    const list = currentList;
    if (list) items.push({ label: "Listenaktionen…", icon: Ellipsis, action: () => openListMenuAt(list, at.anchor) });
    if (currentTag) items.push({ label: "Tag-Aktionen…", icon: Ellipsis, action: () => (tagMenu = { x: at.x, y: at.y }) });
    if (route.view === "completed") {
      items.push({ label: "Aktionen für Erledigt…", icon: Ellipsis, action: () => (completedMenu = { x: at.x, y: at.y }) });
    }
    if (route.view === "trash") {
      items.push({ label: "Papierkorb leeren…", icon: Trash, danger: true, action: () => (emptyTrashConfirm = true) });
    }
    return items;
  }

  function escape(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    if (showPalette) showPalette = false;
    else if (showShortcuts) showShortcuts = false;
    else if (showSettings) showSettings = false;
    else if (appMode.mode === "notes" && notes?.zen) notes.setZen(false);
    else if (target && target.matches("input, textarea, select")) target.blur();
    else if (appMode.mode === "notes") drawerOpen = false;
    else if (openId) closeDetail();
    else if (drawerOpen) drawerOpen = false;
    else if (selecting()) taskList?.exitSelection();
    else selectedId = null;
  }

  shortcuts.setModal(
    () => showShortcuts || showSettings || showNewTask || !!deleteTarget || !!renameDialog || !!duePrompt || showPalette || showNewList || !!cleanup || emptyTrashConfirm || !!tagRename || !!tagDelete || !!taskList?.dialogOpen(),
  );
  shortcuts.setScope(() => appMode.mode);
  // Überall (beide Modi).
  shortcuts.register(
    { keys: ["ctrl+k"], description: "Suchen und Befehle", group: "Allgemein", inInputs: true, run: () => void openPalette() },
    // „/“ nur außerhalb von Feldern – beim Tippen bleibt es ein Zeichen.
    { keys: ["/"], description: "Suchen (außerhalb von Feldern)", group: "Allgemein", run: () => void openPalette() },
    { keys: [], description: "Einstellungen", group: "Allgemein", palette: "Einstellungen öffnen", run: () => void openSettings() },
    // Nur Desktop (ADR-032): Export/Import über System-Dialoge.
    { keys: [], description: "Daten exportieren", group: "Allgemein", palette: "Daten exportieren…", when: () => !!client.data, run: () => void exportAll(client) },
    { keys: [], description: "Aus Export wiederherstellen", group: "Allgemein", palette: "Aus Export wiederherstellen…", when: () => !!client.data, run: () => void importFile(client) },
    {
      keys: [],
      description: "Seitenleiste ein-/ausklappen",
      group: "Allgemein",
      palette: "Seitenleiste ein-/ausklappen",
      run: toggleCollapse,
    },
    {
      keys: ["?"],
      description: "Tastaturkürzel anzeigen",
      group: "Allgemein",
      palette: "Tastaturkürzel anzeigen",
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
    {
      keys: ["ctrl+1"],
      description: "Zu Tasks wechseln",
      group: "Allgemein",
      palette: "Zu Tasks wechseln",
      inInputs: true,
      when: () => appMode.mode !== "tasks",
      run: () => void switchMode("tasks"),
    },
    {
      keys: ["ctrl+2"],
      description: "Zu Notes wechseln",
      group: "Allgemein",
      palette: "Zu Notes wechseln",
      inInputs: true,
      when: () => appMode.mode !== "notes",
      run: () => void switchMode("notes"),
    },
  );
  // Nur im Modus „Tasks" (N5: der Modus „Notes" registriert eigene Kürzel).
  shortcuts.register(
    ...inScope("tasks", [
    {
      keys: ["shift+f10", "contextmenu"],
      label: "Umschalt+F10",
      description: "Listenaktionen (fokussierte Liste)",
      group: "Listen",
      when: () => !!focusedList(),
      run: () => openListMenuAt(focusedList()!, document.activeElement!),
    },
    {
      keys: ["alt+arrowup"],
      description: "Liste nach oben (fokussierte Liste)",
      group: "Listen",
      when: () => !!focusedList(),
      run: () => {
        const list = focusedList()!;
        const i = lists.userLists.indexOf(list);
        if (i > 0) void moveList(list.id, i - 1);
      },
    },
    {
      keys: ["alt+arrowdown"],
      description: "Liste nach unten (fokussierte Liste)",
      group: "Listen",
      when: () => !!focusedList(),
      run: () => {
        const list = focusedList()!;
        const i = lists.userLists.indexOf(list);
        if (i < lists.userLists.length - 1) void moveList(list.id, i + 2);
      },
    },
    { keys: ["n"], description: "Neue Task", group: "Allgemein", palette: "Neue Task", run: newTask },
    { keys: [], description: "Neue Liste…", group: "Listen", palette: "Neue Liste…", run: () => (showNewList = true) },
    ...(
      [
        ["alt+d", "Fälligkeit", () => newTaskDialog?.toggleDue()],
        ["alt+p", "Priorität", () => newTaskDialog?.openPriority()],
        ["alt+w", "Wiederholung", () => newTaskDialog?.openRepeat()],
        ["alt+t", "Tags", () => newTaskDialog?.toggleTags()],
        ["alt+l", "Liste", () => newTaskDialog?.openListPicker()],
        ["alt+n", "Notiz", () => newTaskDialog?.toggleNotes()],
      ] as const
    ).map(([key, what, run]) => ({
      keys: [key],
      description: `${what} (im Fenster „Neue Task“)`,
      group: "Allgemein",
      inInputs: true,
      inModal: true,
      when: () => !!document.activeElement?.closest("[data-new-task]"),
      run,
    })),
    { keys: ["ctrl+n"], description: "Neue Task (auch beim Tippen)", group: "Allgemein", inInputs: true, run: newTask },
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
    {
      keys: ["alt+arrowup"],
      description: "Nach oben verschieben",
      group: "Liste",
      when: () => !selecting() && !!selected(),
      run: () => taskList?.moveSelected(-1),
    },
    {
      keys: ["alt+arrowdown"],
      description: "Nach unten verschieben",
      group: "Liste",
      when: () => !selecting() && !!selected(),
      run: () => taskList?.moveSelected(1),
    },
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
      keys: ["e"],
      description: "Ausgewählte erledigen bzw. wieder öffnen",
      group: "Auswahl",
      when: hasSelection,
      run: () => taskList?.completeOrReopenSelected(),
    },
    {
      keys: ["delete"],
      description: "Ausgewählte löschen (mit Rückgängig)",
      group: "Auswahl",
      when: hasSelection,
      run: () => void taskList?.deleteSelected(),
    },
    { keys: ["m"], description: "Ausgewählte verschieben nach…", group: "Auswahl", when: hasSelection, run: openMoveMenu },
    // Fälligkeit (P4-04): fokussierte Zeile/Karte oder die Auswahl; eine Uhrzeit bleibt.
    {
      keys: ["t"],
      description: "Fällig heute",
      group: "Fälligkeit",
      when: () => dueTargets().length > 0,
      run: () => setDue(dueTargets(), todayPlus(0)),
    },
    {
      keys: ["shift+t"],
      description: "Fällig morgen",
      group: "Fälligkeit",
      when: () => dueTargets().length > 0,
      run: () => setDue(dueTargets(), todayPlus(1)),
    },
    {
      keys: ["d"],
      description: "Fälligkeit wählen…",
      group: "Fälligkeit",
      when: () => dueTargets().length > 0,
      run: () => openDueMenu(),
    },
    {
      keys: ["alt+arrowup"],
      description: "Auswahlblock nach oben",
      group: "Auswahl",
      when: hasSelection,
      run: () => taskList?.moveSelectionBlock(-1),
    },
    {
      keys: ["alt+arrowdown"],
      description: "Auswahlblock nach unten",
      group: "Auswahl",
      when: hasSelection,
      run: () => taskList?.moveSelectionBlock(1),
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
    // Board (P2-06): Navigation und Aktionen auf der Cursor-Karte.
    { keys: ["arrowdown", "j"], label: "↓ / J", description: "Nächste Karte", group: "Board", when: () => showBoard && !!board?.hasCards(), run: () => board?.moveCursor(0, 1) },
    { keys: ["arrowup", "k"], label: "↑ / K", description: "Vorige Karte", group: "Board", when: () => showBoard && !!board?.hasCards(), run: () => board?.moveCursor(0, -1) },
    { keys: ["arrowright", "l"], label: "→ / L", description: "Nächste Spalte", group: "Board", when: () => showBoard && !!board?.hasCards(), run: () => board?.moveCursor(1, 0) },
    { keys: ["arrowleft", "h"], label: "← / H", description: "Vorige Spalte", group: "Board", when: () => showBoard && !!board?.hasCards(), run: () => board?.moveCursor(-1, 0) },
    {
      keys: ["space", "e"],
      label: "Leertaste / E",
      description: "Karte erledigt umschalten",
      group: "Board",
      when: () => showBoard && !!board?.cursorTask(),
      run: () => board?.toggleCursor(),
    },
    { keys: ["enter"], description: "Karte öffnen", group: "Board", when: () => showBoard && !!board?.cursorTask(), run: () => board?.openCursor() },
    {
      keys: ["delete"],
      description: "Karte löschen (mit Rückgängig)",
      group: "Board",
      when: () => showBoard && !!board?.cursorTask(),
      run: () => void board?.deleteCursor(),
    },
    ...([
      ["alt+arrowleft", "Spalte nach links (Spaltenkopf fokussiert)", -1],
      ["alt+arrowright", "Spalte nach rechts (Spaltenkopf fokussiert)", 1],
    ] as const).map(([key, description, dx]) => ({
      keys: [key],
      description,
      group: "Board",
      when: () => showBoard && !!board?.focusedColumn(),
      run: () => void board?.moveFocusedColumn(dx),
    })),
    ...([
      ["alt+arrowleft", "Karte in die Spalte links", -1, 0],
      ["alt+arrowright", "Karte in die Spalte rechts", 1, 0],
      ["alt+arrowup", "Karte nach oben", 0, -1],
      ["alt+arrowdown", "Karte nach unten", 0, 1],
    ] as const).map(([key, description, dx, dy]) => ({
      keys: [key],
      description,
      group: "Board",
      when: () => showBoard && !!board?.cursorTask(),
      run: () => board?.moveCursorCard(dx, dy),
    })),
    {
      keys: ["v"],
      description: "Darstellung Liste/Kanban umschalten",
      group: "Ansichten",
      palette: "Darstellung Liste/Kanban umschalten",
      when: () => boardCapable,
      run: () => setLayout(layout === "list" ? "board" : "list"),
    },
    { keys: ["g i"], description: "Eingang", group: "Ansichten", palette: "Gehe zu Eingang", run: () => goTo("inbox") },
    { keys: ["g t"], description: "Heute", group: "Ansichten", palette: "Gehe zu Heute", run: () => goTo("today") },
    { keys: ["g u"], description: "Geplant", group: "Ansichten", palette: "Gehe zu Geplant", run: () => goTo("upcoming") },
    { keys: [], description: "Alle Tasks", group: "Ansichten", palette: "Gehe zu Alle Tasks", run: () => goTo("all") },
    { keys: [], description: "Erledigt", group: "Ansichten", palette: "Gehe zu Erledigt", run: () => goTo("completed") },
    ]),
  );
</script>

<div
  class="app"
  class:collapsed
  class:drawer-open={drawerOpen}
  class:detail-open={openId !== null && appMode.mode === "tasks"}
  class:notes-mode={appMode.mode === "notes"}
  class:zen={appMode.mode === "notes" && !!notes?.zen}
>
  <aside class="sidebar-slot">
    <Sidebar
      {route}
      {lists}
      collapsed={collapsed && !drawerOpen}
      status={sync.status}
      account={sync.account}
      onnewtask={newTask}
      oncreatelist={createList}
      {renamingId}
      onlistmenu={(list, x, y) => (listMenu = { list, x, y })}
      onrename={renameList}
      onrenamecancel={() => (renamingId = null)}
      onmovelist={(id, to) => void moveList(id, to)}
      ontogglecollapse={toggleCollapse}
      onnavigate={() => (drawerOpen = false)}
      onlogin={() => void openSettings()}
      onsync={() => void openSettings()}
      onsettings={() => void openSettings()}
      onsearch={() => void openPalette()}
      mode={appMode.mode}
      onswitchmode={() => void switchMode(appMode.mode === "tasks" ? "notes" : "tasks")}
    >
      {#snippet notesBody()}
        {#if notesChunk && notes}
          <notesChunk.NotesSidebar
            bind:this={notesSidebar}
            {notes}
            {shortcuts}
            collapsed={collapsed && !drawerOpen}
            onnavigate={() => (drawerOpen = false)}
          />
        {/if}
      {/snippet}
    </Sidebar>
  </aside>
  {#if drawerOpen}
    <button type="button" class="scrim" aria-label="Navigation schließen" onclick={() => (drawerOpen = false)}></button>
  {/if}

  {#if notesChunk && notes}
    <main class="pane notes-pane" aria-label="Notes" hidden={appMode.mode !== "notes"}>
      <notesChunk.NotesMain
        bind:this={notesMain}
        {notes}
        {shortcuts}
        sync={() => sync.status}
        onopennav={() => (drawerOpen = true)}
      />
    </main>
  {/if}
  <main class="pane" aria-labelledby="view-title" hidden={appMode.mode !== "tasks"}>
    <header class="pane-header" class:wide={showBoard}>
      <span class="menu">
        <IconButton label="Navigation öffnen" icon={PanelLeft} onclick={() => (drawerOpen = true)} />
      </span>
      <div class="titles">
        <h1 id="view-title">{title}</h1>
        {#if subtitle}<p class="subtitle">{subtitle}</p>{/if}
        {#if optionChips.length > 0 && !showBoard}
          <div class="option-chips" role="group" aria-label="Aktive Sortierung und Filter">
            {#each optionChips as chip (chip.label)}
              <span class="option-chip">
                {chip.label}
                <button type="button" aria-label="{chip.label} aufheben" onclick={chip.clear}>
                  <X size={12} strokeWidth={2} aria-hidden="true" />
                </button>
              </span>
            {/each}
          </div>
        {/if}
      </div>
      {#if selection?.active && taskList}
        {@const flags = taskList.selectionFlags()}
        <SelectionBar
          all={taskList.selectionAll()}
          count={selection.count}
          ontoggleall={() => void taskList?.toggleAll()}
          ondone={() => taskList?.exitSelection()}
        >
          {#snippet actions()}
            {#if route.view === "trash"}
              <IconButton
                label="Wiederherstellen"
                icon={Undo}
                disabled={selection.count === 0}
                onclick={() => void taskList?.restoreSelected()}
              />
              <IconButton
                label="Endgültig löschen"
                shortcut="Entf"
                icon={Trash}
                disabled={selection.count === 0}
                onclick={() => void taskList?.deleteSelected()}
              />
            {:else}
            <IconButton
              label="Erledigt"
              shortcut="E"
              icon={CircleCheck}
              disabled={!flags.hasOpen}
              onclick={() => void taskList?.completeSelected(true)}
            />
            <IconButton
              label="Wieder öffnen"
              shortcut={flags.hasOpen ? undefined : "E"}
              icon={Undo}
              disabled={!flags.hasDone}
              onclick={() => void taskList?.completeSelected(false)}
            />
            <IconButton
              label="Verschieben nach…"
              shortcut="M"
              icon={FolderInput}
              disabled={selection.count === 0}
              data-move-button
              onclick={openMoveMenu}
            />
            <IconButton
              label="Fälligkeit…"
              shortcut="D"
              icon={CalendarDays}
              disabled={selection.count === 0}
              data-due-button
              onclick={() => openDueMenu()}
            />
            <IconButton
              label="Löschen"
              shortcut="Entf"
              icon={Trash}
              disabled={selection.count === 0}
              onclick={() => void taskList?.deleteSelected()}
            />
            {/if}
          {/snippet}
        </SelectionBar>
      {:else if density.narrow}
        <!-- Schmal (P5-09): Darstellung, Auswählen und Listenaktionen in einem Menü statt Icon-Reihe. -->
        <span class="header-actions">
          <IconButton
            label="Ansicht und Aktionen"
            icon={Ellipsis}
            onclick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              viewMenu = { x: r.left, y: r.bottom + 4, anchor: e.currentTarget };
            }}
          />
        </span>
      {:else}
        <span class="header-actions">
          {#if boardCapable}
            <span class="layout-switch" role="group" aria-label="Darstellung">
              <IconButton
                label="Liste"
                shortcut="V"
                icon={List}
                pressed={layout === "list"}
                onclick={() => setLayout("list")}
              />
              <IconButton
                label="Kanban"
                shortcut="V"
                icon={SquareKanban}
                pressed={layout === "board"}
                onclick={() => setLayout("board")}
              />
            </span>
          {/if}
          {#if !showBoard}
            <IconButton
              label="Sortieren und filtern"
              icon={SlidersHorizontal}
              pressed={!isDefault(viewOptions)}
              aria-haspopup="dialog"
              aria-expanded={optionsOpen}
              onclick={(e) => {
                optionsButton = e.currentTarget;
                optionsOpen = !optionsOpen;
              }}
            />
          {/if}
          <IconButton
            label={showBoard ? "Auswählen (nur in der Listenansicht)" : "Auswählen"}
            shortcut={showBoard ? undefined : "Strg+A"}
            icon={ListChecks}
            disabled={showBoard || !hasList()}
            onclick={() => taskList?.startSelection()}
          />
          {#if currentList}
            <IconButton
              label="Listenaktionen"
              icon={Ellipsis}
              onclick={(e) => openListMenuAt(currentList!, e.currentTarget)}
            />
          {/if}
          {#if currentTag}
            <IconButton
              label="Tag-Aktionen"
              icon={Ellipsis}
              onclick={(e) => {
                const r = e.currentTarget.getBoundingClientRect();
                tagMenu = { x: r.left, y: r.bottom + 4 };
              }}
            />
          {/if}
          {#if route.view === "trash"}
            <IconButton
              label="Aktionen für Papierkorb"
              icon={Ellipsis}
              onclick={(e) => {
                const r = e.currentTarget.getBoundingClientRect();
                trashMenu = { x: r.left, y: r.bottom + 4 };
              }}
            />
          {/if}
          {#if route.view === "completed"}
            <IconButton
              label="Aktionen für Erledigt"
              icon={Ellipsis}
              onclick={(e) => {
                const r = e.currentTarget.getBoundingClientRect();
                completedMenu = { x: r.left, y: r.bottom + 4 };
              }}
            />
          {/if}
        </span>
      {/if}
    </header>
    {#if density.narrow && !selection?.active}
      <button type="button" class="fab" aria-label="Neue Task" data-tooltip="Neue Task   N" data-tooltip-side="left" onclick={newTask}>
        <Plus size={24} strokeWidth={2} aria-hidden="true" />
      </button>
    {/if}
    <div class="pane-body">
      {#if ready && (route.view !== "list" || lists.lists.some((l) => l.id === route.listId))}
        {#key `${routeHash(route)}|${optionsKey(viewOptions)}`}
          <div class="list-slot">
            {#if showBoard}
              <Board
                bind:this={board}
                {lists}
                highlightId={route.view === "list" ? route.listId : route.view === "inbox" ? lists.inbox?.id : undefined}
                bind:selectedId
                onopen={(id) => (openId = id)}
                onready={listReady}
                oncolumnmenu={(list, x, y) => (listMenu = { list, x, y })}
              />
            {:else}
              <TaskList
                bind:this={taskList}
                {route}
                {lists}
                bind:selectedId
                onopen={(id) => (openId = id)}
                onready={listReady}
                onnewtask={newTask}
                ondue={(tasks, x, y) => openDueMenu(tasks, { x, y })}
                options={viewOptions}
              />
            {/if}
          </div>
        {/key}
      {/if}
    </div>
  </main>

  {#if openId && appMode.mode === "tasks" && TaskDetail}
    <aside class="detail-slot">
      <TaskDetail
        taskId={openId}
        {lists}
        onclose={closeDetail}
        ondelete={deleteFromDetail}
        onopennote={(id) => void openNote(id, null)}
      />
    </aside>
  {/if}
</div>
<Toaster />
<Tooltips />
{#if showNewTask}
  <NewTaskDialog
    bind:this={newTaskDialog}
    {lists}
    listId={routeListId(route)}
    shownListId={showBoard
      ? undefined
      : route.view === "list"
        ? route.listId
        : route.view === "inbox"
          ? lists.inbox?.id
          : undefined}
    onopenlist={(id) => router.navigate(id === lists.inbox?.id ? { view: "inbox" } : { view: "list", listId: id })}
    defaultDue={route.view === "today" ? todayPlus(0) : null}
    defaultTags={currentTag ? [currentTag.name] : []}
    initialText={newTaskText}
    onclose={() => (showNewTask = false)}
  />
{/if}
{#if showPalette && CommandPalette}
  <CommandPalette
    {lists}
    commands={paletteCommands}
    recent={recentTasks}
    onopentask={openFromPalette}
    onopenlist={(list) => {
      router.navigate(list.isInbox ? { view: "inbox" } : { view: "list", listId: list.id });
      selectedId = openId = null;
    }}
    onnewtask={(text) => {
      newTaskText = text;
      showNewTask = true;
    }}
    mode={appMode.mode}
    onopennote={(id, hit) => void openNote(id, hit)}
    onclose={() => (showPalette = false)}
  />
{/if}
{#if viewMenu}
  {@const at = viewMenu}
  <Menu label="Ansicht und Aktionen" items={viewMenuItems(at)} x={at.x} y={at.y} onclose={() => (viewMenu = null)} />
{/if}
{#if trashMenu}
  <Menu
    label="Aktionen für Papierkorb"
    items={[
      {
        label: "Papierkorb leeren…",
        icon: Trash,
        danger: true,
        action: () => (emptyTrashConfirm = true),
      },
    ]}
    x={trashMenu.x}
    y={trashMenu.y}
    onclose={() => (trashMenu = null)}
  />
{/if}
{#if emptyTrashConfirm}
  <PurgeDialog
    subject={lists.counts.trash > 0
      ? `alle ${lists.counts.trash === 1 ? "1 Task" : `${lists.counts.trash} Tasks`} im Papierkorb`
      : "alle Tasks im Papierkorb"}
    onconfirm={() => void emptyTrash()}
    onclose={() => (emptyTrashConfirm = false)}
  />
{/if}
{#if completedMenu}
  <Menu
    label="Aktionen für Erledigt"
    items={[
      {
        label: `Erledigte älter als ${CLEANUP_DAYS} Tage löschen…`,
        icon: Trash,
        danger: true,
        action: () => void startCleanup(),
      },
    ]}
    x={completedMenu.x}
    y={completedMenu.y}
    onclose={() => (completedMenu = null)}
  />
{/if}
{#if cleanup}
  <CleanupCompletedDialog
    count={cleanup.ids.length}
    cutoff={cleanup.cutoff}
    onconfirm={confirmCleanup}
    onclose={() => (cleanup = null)}
  />
{/if}
{#if showNewList}
  <ListNameDialog
    title="Neue Liste"
    submitLabel="Anlegen"
    onsubmit={(name) => createList(name)}
    onclose={() => (showNewList = false)}
  />
{/if}
{#if listMenu}
  <Menu
    label="Aktionen für Liste {listMenu.list.name}"
    items={listMenuItems(listMenu.list)}
    x={listMenu.x}
    y={listMenu.y}
    onclose={() => (listMenu = null)}
  />
{/if}
{#if dueMenu}
  <Menu
    label="Fälligkeit"
    items={dueMenuItems(dueMenu.targets)}
    x={dueMenu.x}
    y={dueMenu.y}
    onclose={() => (dueMenu = null)}
  />
{/if}
{#if duePrompt}
  {@const targets = duePrompt}
  <DuePrompt
    subject={targets.length === 1 ? `„${targets[0].title}“` : `${targets.length} Tasks`}
    onsubmit={(due) => setDue(targets, due)}
    onclose={() => (duePrompt = null)}
  />
{/if}
{#if deleteTarget}
  <DeleteListDialog list={deleteTarget} {lists} onconfirm={() => void confirmDelete()} onclose={() => (deleteTarget = null)} />
{/if}
{#if optionsOpen && optionsButton}
  <ViewOptionsPopover
    anchor={optionsButton}
    {route}
    options={viewOptions}
    tags={lists.tags}
    onchange={setOptions}
    onclose={(refocus) => {
      optionsOpen = false;
      if (refocus) optionsButton?.focus();
    }}
  />
{/if}
{#if mergeMenu}
  {@const source = mergeMenu.list}
  <Menu
    label="„{source.name}“ zusammenführen mit"
    items={mergeTargets(source)}
    x={mergeMenu.x}
    y={mergeMenu.y}
    onclose={() => (mergeMenu = null)}
  />
{/if}
{#if tagMenu && currentTag}
  <Menu
    label="Aktionen für Tag {currentTag.name}"
    items={tagMenuItems(currentTag)}
    x={tagMenu.x}
    y={tagMenu.y}
    onclose={() => (tagMenu = null)}
  />
{/if}
{#if tagRename}
  <ListNameDialog
    title="Tag umbenennen"
    submitLabel="Umbenennen"
    initial={tagRename.name}
    onsubmit={(name) => renameTag(tagRename!, name)}
    onclose={() => (tagRename = null)}
  />
{/if}
{#if tagDelete}
  <Dialog title="Tag löschen?" onclose={() => (tagDelete = null)} width={400}>
    <p class="confirm-text">„#{tagDelete.name}“ wird von allen Tasks entfernt. Die Tasks selbst bleiben.</p>
    <div class="confirm-actions">
      <Button onclick={() => (tagDelete = null)}>Abbrechen</Button>
      <Button variant="danger" onclick={() => void confirmTagDelete()}>Tag löschen</Button>
    </div>
  </Dialog>
{/if}
{#if renameDialog}
  <ListNameDialog
    title="Liste umbenennen"
    submitLabel="Umbenennen"
    initial={renameDialog.name}
    onsubmit={(name) => renameList(renameDialog!, name)}
    onclose={() => (renameDialog = null)}
  />
{/if}
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
  .pane[hidden] {
    display: none;
  }
  .pane {
    animation: mode-fade var(--duration-base) var(--ease);
  }
  @keyframes mode-fade {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pane {
      animation: none;
    }
  }
  .pane-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 80px;
    /* Gleiche Kanten wie die Zeilen der Lesespalte (Einzug 8 px + Innenabstand der Zeile). */
    padding: 0 calc(var(--content-inset) + var(--space-6)) 0 calc(var(--content-inset) + var(--space-8));
    flex: none;
  }
  .pane-header.wide {
    padding: 0 var(--space-4) 0 var(--space-6);
  }
  .layout-switch {
    display: flex;
    gap: 2px;
    margin-right: var(--space-2);
    padding: 2px;
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
  }
  .layout-switch :global(.icon-btn) {
    width: 28px;
    height: 28px;
  }
  /* Schmal (P5-09): „Neue Task“ als runder Button unten rechts, über der Liste. */
  .fab {
    position: fixed;
    right: var(--space-4);
    bottom: calc(var(--space-4) + env(safe-area-inset-bottom, 0px));
    z-index: 25;
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    border: 0;
    border-radius: var(--radius-full);
    background: var(--color-accent-fill);
    color: var(--color-on-accent);
    box-shadow: var(--shadow-popover);
    cursor: pointer;
  }
  .fab:hover {
    background: var(--color-accent-fill-hover);
  }
  .fab:focus-visible {
    outline-offset: 3px;
  }
  .header-actions {
    display: flex;
    gap: var(--space-1);
    margin-left: auto;
  }
  .titles {
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
  }
  .subtitle {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  h1 {
    margin: 0;
    font-size: var(--text-2xl);
    line-height: 1.25;
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

  /* Zen (Notes): keine Seitenleiste, der Editor füllt das Fenster. */
  .app.zen {
    grid-template-columns: minmax(0, 1fr);
  }
  .app.zen .sidebar-slot {
    display: none;
  }
  .app.zen .notes-pane {
    margin: 0;
    border: 0;
    border-radius: 0;
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
  .confirm-text {
    margin: 0 0 var(--space-4);
    line-height: 1.5;
  }
  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .option-chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }
  .option-chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 22px;
    padding: 0 2px 0 var(--space-2);
    border-radius: var(--radius-md);
    background: var(--color-bg-selected);
    color: var(--color-accent-text);
    font-size: var(--text-xs);
  }
  .option-chip button {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
</style>
