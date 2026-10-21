<script lang="ts">
  /**
   * Statuszeile des Editors (`notes.md` §6.1): Pfad und Name (Klick benennt um), Speicher- und
   * Sync-Zustand, Wörter, Zeichen, Auswahl, verknüpfte Tasks (N9-02, ADR-036), `⋯`-Menü. Zähler kommen
   * gebündelt vom Editor.
   */
  import type { LinkedTaskDto, NoteDto, SyncStatusDto } from "@tasks/core-client";
  import type { Snippet } from "svelte";
  import { onMount, tick } from "svelte";
  import { toasts } from "../../components/toasts.svelte";
  import IconButton from "../../components/IconButton.svelte";
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import {
    Circle,
    CircleCheck,
    Ellipsis,
    FileCode,
    FileText,
    FolderInput,
    ListChecks,
    Maximize,
    Minimize,
    Pencil,
    Plus,
    Trash,
  } from "../../icons";
  import FolderPicker from "./FolderPicker.svelte";
  import { move, rename, setKind, trashWithUndo } from "./notes-actions";
  import type { NotesState } from "./notes-state.svelte";

  interface Props {
    notes: NotesState;
    note: NoteDto;
    readonly: boolean;
    counts: { words: number; chars: number; selected: number };
    large: boolean;
    saving: boolean;
    sync: SyncStatusDto;
    /** Rechts neben den Zählern (z. B. Umschalter Bearbeiten/Vorschau). */
    extra?: Snippet;
  }

  let { notes, note, readonly, counts, large, saving, sync, extra }: Props = $props();
  let renaming = $state(false);
  let input = $state<HTMLInputElement>();
  let menu = $state<{ x: number; y: number } | null>(null);
  let picking = $state(false);

  // Verknüpfte Tasks (ADR-036): Zähler in der Zeile, Menü mit Öffnen und „Neue Task“.
  let linkedTasks = $state.raw<LinkedTaskDto[]>([]);
  let tasksMenu = $state<{ x: number; y: number } | null>(null);
  const client = $derived(notes.client);

  async function loadLinked(id: string) {
    linkedTasks = await client.noteLinks(id).catch(() => []);
  }

  $effect(() => {
    void loadLinked(note.id);
  });

  onMount(() =>
    client.onChange((e) => {
      if (e.links.length > 0 || e.tasks.length > 0) void loadLinked(note.id);
    }),
  );

  /** Neue Task mit dem Namen der Notiz, gleich verknüpft (Eingang). */
  async function newTaskFromNote() {
    try {
      const task = await client.createTask({ title: note.name });
      await client.linkTaskNote(task.id, note.id);
      toasts.show({
        message: `Task „${task.title}“ angelegt und verknüpft`,
        actionLabel: notes.openTask ? "Öffnen" : undefined,
        onAction: notes.openTask ? () => notes.openTask?.(task.id) : undefined,
      });
    } catch (e) {
      toasts.show({ message: `Nicht angelegt: ${(e as Error).message}`, tone: "danger" });
    }
  }

  function taskItems(): MenuItem[] {
    return [
      ...linkedTasks.map((l) => ({
        id: l.linkId,
        label: l.task.title,
        icon: l.task.status === "done" ? CircleCheck : Circle,
        action: () => notes.openTask?.(l.task.id),
      })),
      { id: "new", label: "Neue Task aus dieser Notiz", icon: Plus, action: () => void newTaskFromNote() },
    ];
  }

  const number = new Intl.NumberFormat("de-DE");
  const ext = $derived(note.kind === "markdown" ? ".md" : ".txt");
  const path = $derived(notes.path(note.folderId));
  const saveState = $derived.by(() => {
    if (readonly) return "Im Papierkorb – nur lesen";
    if (saving) return "Speichert…";
    const waiting = sync.pendingOps > 0;
    if (sync.state === "disabled") return "Gespeichert";
    if (waiting && ["offline", "error", "authRequired", "rateLimited", "newerFormat"].includes(sync.state)) {
      return "Nicht synchronisiert";
    }
    if (!waiting && sync.state === "idle") return "Synchronisiert";
    return "Gespeichert";
  });

  async function startRename() {
    if (readonly) return;
    renaming = true;
    await tick();
    input?.focus();
    input?.select();
  }

  async function submit() {
    const name = input?.value ?? "";
    renaming = false;
    await rename(notes, { type: "note", ...summaryOf(note) }, name, note.folderId);
  }

  function summaryOf(n: NoteDto) {
    return {
      id: n.id,
      folderId: n.folderId,
      name: n.name,
      kind: n.kind,
      sortKey: n.sortKey,
      createdAt: n.createdAt,
      modifiedAt: n.modifiedAt,
      trashedAt: n.trashedAt,
      size: 0,
    };
  }

  function items(): MenuItem[] {
    const list: MenuItem[] = [];
    if (!readonly) {
      list.push(
        { label: "Umbenennen", icon: Pencil, action: () => void startRename() },
        note.kind === "markdown"
          ? { label: "Typ: Text (.txt)", icon: FileText, action: () => void setKind(notes, note.id, "text") }
          : { label: "Typ: Markdown (.md)", icon: FileCode, action: () => void setKind(notes, note.id, "markdown") },
        { label: "Verschieben nach…", icon: FolderInput, action: () => (picking = true) },
        {
          label: "In den Papierkorb",
          icon: Trash,
          danger: true,
          action: () => void trashWithUndo(notes, [{ type: "note", ...summaryOf(note) }]),
        },
      );
    }
    list.push({
      label: notes.zen ? "Zen-Modus beenden" : "Zen-Modus",
      icon: notes.zen ? Minimize : Maximize,
      shortcut: "Strg+Umschalt+F",
      action: () => notes.setZen(!notes.zen),
    });
    return list;
  }
</script>

<footer class="status" aria-label="Status der Notiz">
  <span class="where">
    {#if path.length > 0}<span class="path">{path.join(" › ")} ›</span>{/if}
    {#if renaming}
      <input
        bind:this={input}
        class="name-input"
        aria-label="Neuer Name der Notiz"
        value={note.name}
        spellcheck="false"
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void submit();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            renaming = false;
          }
        }}
        onblur={() => (renaming = false)}
      />
    {:else}
      <button type="button" class="name" title="Umbenennen" disabled={readonly} onclick={() => void startRename()}>
        {note.name}{ext}
      </button>
    {/if}
  </span>
  <span class="facts" aria-live="polite">
    <span class:warn={large}>{large ? "Große Notiz (> 256 KB) · " : ""}{saveState}</span>
    <span>· {number.format(counts.words)} {counts.words === 1 ? "Wort" : "Wörter"}</span>
    <span>· {number.format(counts.chars)} Z.</span>
    {#if counts.selected > 0}<span>· {number.format(counts.selected)} ausgewählt</span>{/if}
  </span>
  {#if extra}{@render extra()}{/if}
  <button
    type="button"
    class="tasks"
    aria-haspopup="menu"
    aria-expanded={tasksMenu !== null}
    data-tooltip="Verknüpfte Tasks"
    data-tooltip-side="top"
    onclick={(e) => {
      const r = e.currentTarget.getBoundingClientRect();
      tasksMenu = { x: r.left, y: r.top - 8 };
    }}
  >
    <ListChecks size={14} strokeWidth={1.75} aria-hidden="true" />{linkedTasks.length === 0
      ? "Tasks"
      : linkedTasks.length === 1
        ? "1 Task"
        : `${linkedTasks.length} Tasks`}
  </button>
  <IconButton
    label="Weitere Aktionen"
    icon={Ellipsis}
    size={28}
    tooltipSide="top"
    onclick={(e) => {
      const r = e.currentTarget.getBoundingClientRect();
      menu = { x: r.left, y: r.top - 8 };
    }}
  />
</footer>

{#if tasksMenu}
  <Menu label="Verknüpfte Tasks" items={taskItems()} x={tasksMenu.x} y={tasksMenu.y} onclose={() => (tasksMenu = null)} />
{/if}
{#if menu}
  <Menu label="Aktionen für die Notiz" items={items()} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}
{#if picking}
  <FolderPicker
    {notes}
    subject="„{note.name}“"
    currentFolder={note.folderId}
    onpick={(folderId) => void move(notes, { type: "note", ...summaryOf(note) }, { folderId })}
    onclose={() => (picking = false)}
  />
{/if}

<style>
  .status {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 28px;
    padding: 0 var(--space-2) 0 var(--space-4);
    border-top: 1px solid var(--color-border);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    flex: none;
    white-space: nowrap;
    transition: opacity var(--duration-base) var(--ease);
  }
  .where {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    flex: 1;
    overflow: hidden;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tasks {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .tasks:hover,
  .tasks[aria-expanded="true"] {
    background: var(--color-bg-hover);
    color: var(--color-text);
  }
  .name {
    padding: 0 var(--space-1);
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--color-text);
    font: inherit;
    cursor: text;
  }
  .name:hover:not(:disabled) {
    background: var(--color-bg-hover);
  }
  .name:disabled {
    cursor: default;
  }
  .name-input {
    height: 22px;
    padding: 0 var(--space-1);
    border: 1px solid var(--color-accent);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }
  .facts {
    display: flex;
    gap: var(--space-1);
    font-variant-numeric: tabular-nums;
  }
  .warn {
    color: var(--color-warning);
  }
  @media (max-width: 699px) {
    .facts span:not(:first-child) {
      display: none;
    }
  }
</style>
