<script lang="ts">
  /**
   * Statuszeile des Editors (`notes.md` §6.1): Pfad und Name (Klick benennt um), Speicher- und
   * Sync-Zustand, Wörter, Zeichen, Auswahl, `⋯`-Menü. Zähler kommen gebündelt vom Editor.
   */
  import type { NoteDto, SyncStatusDto } from "@tasks/core-client";
  import type { Snippet } from "svelte";
  import { tick } from "svelte";
  import IconButton from "../../components/IconButton.svelte";
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import { Ellipsis, FileCode, FileText, FolderInput, Maximize, Minimize, Pencil, Trash } from "../../icons";
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
