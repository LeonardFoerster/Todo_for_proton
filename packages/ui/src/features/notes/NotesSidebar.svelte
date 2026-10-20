<script lang="ts">
  /**
   * Seitenleiste im Modus „Notes" (`notes.md` §5.3): „Neue Notiz", Ordnerbaum mit Notizen,
   * Papierkorb. Kopf (Logo, Umschalter) und Sync-Fußzeile gehören `Sidebar`.
   */
  import { onMount, tick } from "svelte";
  import Button from "../../components/Button.svelte";
  import IconButton from "../../components/IconButton.svelte";
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { Ellipsis, FileCode, FilePlus, FileText, FolderInput, FolderPlus, Import, Pencil, Plus, Trash } from "../../icons";
  import type { ShortcutRegistry } from "../../lib/shortcuts";
  import FolderPicker from "./FolderPicker.svelte";
  import ImportResultDialog from "./ImportResultDialog.svelte";
  import { move, newFolder, rename, setKind, trashWithUndo } from "./notes-actions";
  import type { NotesState } from "./notes-state.svelte";
  import NoteTree from "./NoteTree.svelte";
  import { parentIndex, siblingTarget, type DropTarget, type TreeRow } from "./tree-model";

  interface Props {
    notes: NotesState;
    shortcuts: ShortcutRegistry;
    collapsed: boolean;
    onnavigate?: () => void;
  }

  let { notes, shortcuts, collapsed, onnavigate }: Props = $props();
  /** Ordner importieren gibt es nur, wo die Shell es kann (Desktop). */
  const canImport = $derived(!!notes.client.notes.importFolder);

  let tree = $state<NoteTree>();
  let renaming = $state<string | null>(null);
  let menu = $state<{ items: MenuItem[]; label: string; x: number; y: number } | null>(null);
  let picker = $state<{ row: TreeRow } | null>(null);

  const cursorIndex = () => notes.rows.findIndex((r) => r.key === notes.cursor);
  const cursorRow = () => notes.rows[cursorIndex()];
  const treeFocused = () => !!document.activeElement?.closest("[data-note-tree]") && !renaming;

  function activate(row: TreeRow) {
    if (row.item.type === "folder") void notes.toggle(row.item.id);
    else {
      notes.open(row.item.id);
      onnavigate?.();
    }
  }

  function moveCursor(delta: number) {
    const rows = notes.rows;
    if (rows.length === 0) return;
    const i = cursorIndex();
    const next = i < 0 ? (delta > 0 ? 0 : rows.length - 1) : Math.max(0, Math.min(rows.length - 1, i + delta));
    notes.cursor = rows[next].key;
    void tree?.focusCursor();
  }

  async function right() {
    const row = cursorRow();
    if (!row || row.item.type !== "folder") return;
    if (!row.expanded) await notes.expand(row.item.id);
    else {
      const next = notes.rows[cursorIndex() + 1];
      if (next?.parentId === row.item.id) notes.cursor = next.key;
    }
    void tree?.focusCursor();
  }

  function left() {
    const row = cursorRow();
    if (!row) return;
    if (row.item.type === "folder" && row.expanded) notes.collapse(row.item.id);
    else {
      const parent = parentIndex(notes.rows, cursorIndex());
      if (parent >= 0) notes.cursor = notes.rows[parent].key;
    }
    void tree?.focusCursor();
  }

  async function submitName(row: TreeRow, name: string) {
    renaming = null;
    await rename(notes, row.item, name, row.parentId);
    await tick();
    void tree?.focusCursor();
  }

  function drop(row: TreeRow, target: DropTarget) {
    if (target.kind === "into") void move(notes, row.item, { folderId: target.folderId });
    else void move(notes, row.item, { folderId: target.folderId, after: target.after, before: target.before });
  }

  function reorder(delta: -1 | 1) {
    const row = cursorRow();
    const target = row && siblingTarget(notes.rows, cursorIndex(), delta);
    if (row && target) void move(notes, row.item, target).then(() => tree?.focusCursor());
  }

  async function createFolderIn(parentId: string | null) {
    const id = await newFolder(notes, parentId);
    if (!id) return;
    await tick();
    renaming = `f:${id}`;
  }

  /** Ordner, in dem „hier" angelegt wird: der Ordner selbst bzw. der Ordner der Notiz. */
  const placeOf = (row: TreeRow) => (row.item.type === "folder" ? row.item.id : row.parentId);

  function rowMenu(row: TreeRow): MenuItem[] {
    const item = row.item;
    const items: MenuItem[] = [
      { label: "Neue Notiz hier", icon: FilePlus, action: () => void notes.createNote(placeOf(row)) },
      { label: "Neuer Ordner hier", icon: FolderPlus, action: () => void createFolderIn(placeOf(row)) },
      { label: "Umbenennen", icon: Pencil, shortcut: "F2", action: () => (renaming = row.key) },
    ];
    if (item.type === "note") {
      items.push(
        item.kind === "markdown"
          ? { label: "Typ: Text (.txt)", icon: FileText, action: () => void setKind(notes, item.id, "text") }
          : { label: "Typ: Markdown (.md)", icon: FileCode, action: () => void setKind(notes, item.id, "markdown") },
      );
    }
    items.push(
      { label: "Verschieben nach…", icon: FolderInput, shortcut: "M", action: () => (picker = { row }) },
      { label: "In den Papierkorb", icon: Trash, shortcut: "Entf", danger: true, action: () => void trashWithUndo(notes, [item]) },
    );
    return items;
  }

  function openRowMenu(row: TreeRow, x: number, y: number) {
    menu = { items: rowMenu(row), label: `Aktionen für ${row.item.name}`, x, y };
  }

  function openMoreMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = [{ label: "Neuer Ordner", icon: FolderPlus, action: () => void createFolderIn(null) }];
    if (canImport) items.push({ label: "Ordner importieren…", icon: Import, action: () => void notes.importFolder() });
    menu = { items, label: "Weitere Aktionen", x: r.left, y: r.bottom + 4 };
  }

  function rowElement(row: TreeRow): Element | null {
    return document.querySelector(`[data-tree-key="${CSS.escape(row.key)}"]`);
  }

  onMount(() =>
    shortcuts.register(
      ...(
        [
          [["arrowdown", "j"], "↓ / J", "Nächstes Element", () => moveCursor(1)],
          [["arrowup", "k"], "↑ / K", "Voriges Element", () => moveCursor(-1)],
          [["arrowright"], "→", "Ordner aufklappen / hinein", () => void right()],
          [["arrowleft"], "←", "Ordner zuklappen / zum Elternordner", left],
        ] as const
      ).map(([keys, label, description, run]) => ({
        keys: [...keys],
        label,
        description,
        group: "Notizen",
        scope: "notes",
        when: treeFocused,
        run,
      })),
      {
        keys: ["enter"],
        description: "Notiz öffnen und bearbeiten / Ordner auf- und zuklappen",
        group: "Notizen",
        scope: "notes",
        when: () => treeFocused() && !!cursorRow(),
        run: () => {
          const row = cursorRow()!;
          activate(row);
          if (row.item.type === "note") notes.focusEditor();
        },
      },
      {
        keys: ["f2"],
        description: "Umbenennen",
        group: "Notizen",
        scope: "notes",
        when: () => treeFocused() && !!cursorRow(),
        run: () => (renaming = cursorRow()!.key),
      },
      {
        keys: ["delete"],
        description: "In den Papierkorb (mit Rückgängig)",
        group: "Notizen",
        scope: "notes",
        when: () => treeFocused() && !!cursorRow(),
        run: () => void trashWithUndo(notes, [cursorRow()!.item]),
      },
      {
        keys: ["alt+arrowup"],
        description: "Nach oben verschieben",
        group: "Notizen",
        scope: "notes",
        when: () => treeFocused() && !!cursorRow(),
        run: () => reorder(-1),
      },
      {
        keys: ["alt+arrowdown"],
        description: "Nach unten verschieben",
        group: "Notizen",
        scope: "notes",
        when: () => treeFocused() && !!cursorRow(),
        run: () => reorder(1),
      },
      {
        keys: ["m"],
        description: "Verschieben nach…",
        group: "Notizen",
        scope: "notes",
        when: () => treeFocused() && !!cursorRow(),
        run: () => (picker = { row: cursorRow()! }),
      },
      {
        keys: ["shift+f10", "contextmenu"],
        label: "Umschalt+F10",
        description: "Aktionen für das Element",
        group: "Notizen",
        scope: "notes",
        when: () => treeFocused() && !!cursorRow(),
        run: () => {
          const row = cursorRow()!;
          const r = rowElement(row)?.getBoundingClientRect();
          openRowMenu(row, r?.left ?? 0, (r?.bottom ?? 0) + 4);
        },
      },
      ...(canImport
        ? [
            {
              keys: [],
              description: "Ordner importieren…",
              group: "Notizen",
              scope: "notes",
              palette: "Ordner importieren…",
              run: () => void notes.importFolder(),
            },
          ]
        : []),
      {
        keys: [],
        description: "Neuer Ordner",
        group: "Notizen",
        scope: "notes",
        palette: "Neuer Ordner",
        run: () => void createFolderIn(cursorRow() ? placeOf(cursorRow()!) : null),
      },
    ),
  );

  /** Fokus in den Baum (z. B. nach dem Moduswechsel). */
  export function focusTree() {
    if (!notes.cursor && notes.rows[0]) notes.cursor = notes.rows[0].key;
    void tree?.focusCursor();
  }
</script>

<div class="notes-sidebar" class:collapsed>
  <div class="primary">
    {#if collapsed}
      <button
        type="button"
        class="new-compact"
        aria-label="Neue Notiz"
        data-tooltip="Neue Notiz   N"
        data-tooltip-side="right"
        onclick={() => void notes.createNote()}
      >
        <Plus size={20} strokeWidth={2} aria-hidden="true" />
      </button>
    {:else}
      <Button variant="primary" size="lg" block icon={Plus} onclick={() => void notes.createNote()} title="Neue Notiz (N)">
        Neue Notiz
      </Button>
      <IconButton label="Weitere Aktionen" icon={Ellipsis} onclick={openMoreMenu} />
    {/if}
  </div>

  {#if !collapsed}
    <div class="tree-slot">
      {#if notes.loaded && notes.rows.length === 0}
        <p class="hint">Noch keine Notizen – mit „Neue Notiz" anlegen.</p>
      {:else}
        <NoteTree
          bind:this={tree}
          {notes}
          {renaming}
          onactivate={activate}
          onrename={(row) => (renaming = row.key)}
          onsubmitname={(row, name) => void submitName(row, name)}
          oncancelname={() => (renaming = null)}
          onmenu={openRowMenu}
          ondrop={drop}
        />
      {/if}
    </div>
  {/if}

  <div class="footer-nav">
    <button
      type="button"
      class="trash"
      class:active={notes.showTrash}
      aria-current={notes.showTrash ? "page" : undefined}
      aria-label={collapsed ? `Papierkorb, ${notes.trash.length}` : undefined}
      data-tooltip={collapsed ? "Papierkorb" : undefined}
      data-tooltip-side="right"
      onclick={() => {
        notes.showTrash = true;
        onnavigate?.();
      }}
    >
      <Trash size={16} strokeWidth={1.75} aria-hidden="true" />
      {#if !collapsed}
        <span class="label">Papierkorb</span>
        {#if notes.trash.length > 0}<span class="count">{notes.trash.length}</span>{/if}
      {/if}
    </button>
  </div>
</div>

{#if menu}
  <Menu label={menu.label} items={menu.items} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}
{#if notes.importReport}
  <ImportResultDialog report={notes.importReport} onclose={() => (notes.importReport = null)} />
{/if}
{#if picker}
  {@const row = picker.row}
  <FolderPicker
    {notes}
    subject={row.item.type === "folder" ? `Ordner „${row.item.name}“` : `„${row.item.name}“`}
    movingFolder={row.item.type === "folder" ? row.item.id : undefined}
    currentFolder={row.parentId}
    onpick={(folderId) => void move(notes, row.item, { folderId }).then(() => toasts.show({ message: `„${row.item.name}“ verschoben` }))}
    onclose={() => (picker = null)}
  />
{/if}

<style>
  .notes-sidebar {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .primary {
    display: flex;
    gap: var(--space-1);
    align-items: center;
    padding: var(--space-1) var(--space-3) var(--space-3);
  }
  .collapsed .primary {
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
  .tree-slot {
    flex: 1;
    min-height: 0;
  }
  .hint {
    margin: 0;
    padding: var(--space-1) var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    line-height: 1.4;
  }
  .footer-nav {
    padding: var(--space-2);
    border-top: 1px solid var(--color-border);
  }
  .collapsed .footer-nav {
    display: flex;
    justify-content: center;
    border-top: 0;
  }
  .trash {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-md);
    cursor: pointer;
  }
  .collapsed .trash {
    width: 40px;
    justify-content: center;
    padding: 0;
  }
  .trash:hover {
    background: var(--color-bg-hover);
  }
  .trash.active {
    background: var(--color-bg-selected);
    color: var(--color-accent-text);
  }
  .trash .label {
    flex: 1;
    text-align: left;
  }
  .count {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
