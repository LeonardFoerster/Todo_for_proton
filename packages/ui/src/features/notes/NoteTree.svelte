<script lang="ts">
  import { tick } from "svelte";
  import VirtualList from "../../components/VirtualList.svelte";
  import type { NotesState } from "./notes-state.svelte";
  import NoteTreeRow from "./NoteTreeRow.svelte";
  import { dropTarget, type DropTarget, type TreeRow } from "./tree-model";

  interface Props {
    notes: NotesState;
    /** Zeile, die gerade umbenannt wird. */
    renaming: string | null;
    onactivate: (row: TreeRow) => void;
    onrename: (row: TreeRow) => void;
    onsubmitname: (row: TreeRow, name: string) => void;
    oncancelname: () => void;
    onmenu: (row: TreeRow, x: number, y: number) => void;
    /** Gezogene Zeile fallen gelassen. */
    ondrop: (row: TreeRow, target: DropTarget) => void;
  }

  let { notes, renaming, onactivate, onrename, onsubmitname, oncancelname, onmenu, ondrop }: Props = $props();

  const ROW = 32;
  let list = $state<VirtualList<TreeRow>>();
  let drag = $state<{ row: TreeRow; target: DropTarget | null; active: boolean } | null>(null);

  const activeKey = $derived(notes.openId && !notes.showTrash ? `n:${notes.openId}` : null);

  /** Fokus auf die Cursor-Zeile (nach Tastatur-Navigation). */
  export async function focusCursor() {
    const index = notes.rows.findIndex((r) => r.key === notes.cursor);
    if (index < 0) return;
    list?.scrollToIndex(index);
    await tick();
    list
      ?.element()
      .querySelector<HTMLElement>(`[data-tree-key="${CSS.escape(notes.cursor!)}"]`)
      ?.focus({ preventScroll: true });
  }

  export function scrollToIndex(index: number) {
    list?.scrollToIndex(index);
  }

  /** Ziehen beginnt erst nach 4 px (sonst ist es ein Klick). */
  function pointerdown(row: TreeRow, e: PointerEvent) {
    if (e.button !== 0) return;
    const startY = e.clientY;
    const startX = e.clientX;
    drag = { row, target: null, active: false };
    const viewport = list?.element();
    const move = (ev: PointerEvent) => {
      if (!drag || !viewport) return;
      if (!drag.active && Math.hypot(ev.clientX - startX, ev.clientY - startY) < 4) return;
      const rect = viewport.getBoundingClientRect();
      const y = ev.clientY - rect.top + viewport.scrollTop;
      drag = { ...drag, active: true, target: dropTarget(notes.rows, y, ROW, row.key) };
    };
    const end = (ev: PointerEvent) => {
      removeEventListener("pointermove", move);
      removeEventListener("pointerup", end);
      removeEventListener("pointercancel", end);
      const done = drag;
      drag = null;
      if (done?.active && done.target && ev.type === "pointerup") ondrop(done.row, done.target);
    };
    addEventListener("pointermove", move);
    addEventListener("pointerup", end);
    addEventListener("pointercancel", end);
  }
</script>

<div class="tree" data-note-tree>
  <VirtualList
    bind:this={list}
    items={notes.rows}
    key={(r) => r.key}
    rowHeight={ROW}
    role="tree"
    label="Notizen"
  >
    {#snippet row(r)}
      <NoteTreeRow
        row={r}
        cursor={notes.cursor === r.key || (!notes.cursor && r === notes.rows[0])}
        active={activeKey === r.key}
        renaming={renaming === r.key}
        dragging={!!drag?.active && drag.row.key === r.key}
        dropInto={!!drag?.active && drag.target?.kind === "into" && drag.target.folderId === r.item.id}
        onactivate={(row) => {
          notes.cursor = row.key;
          onactivate(row);
        }}
        {onrename}
        {onsubmitname}
        {oncancelname}
        oncontextmenu={(row, e) => {
          notes.cursor = row.key;
          onmenu(row, e.clientX, e.clientY);
        }}
        onpointerdown={pointerdown}
        onfocus={(row) => (notes.cursor = row.key)}
      />
    {/snippet}
    {#snippet overlay()}
      {#if drag?.active && drag.target?.kind === "between"}
        <div class="drop-line" style:top="{drag.target.line - 1}px" aria-hidden="true"></div>
      {/if}
    {/snippet}
  </VirtualList>
</div>

<style>
  .tree {
    height: 100%;
    min-height: 0;
  }
  .drop-line {
    position: absolute;
    left: var(--space-3);
    right: var(--space-3);
    height: 2px;
    border-radius: 1px;
    background: var(--color-accent);
    pointer-events: none;
  }
</style>
