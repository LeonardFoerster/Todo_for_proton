<script lang="ts">
  /**
   * Checkliste im Detail (N8-01, ADR-034): Punkte abhaken, umbenennen (Enter/Verlassen speichert),
   * löschen, mit `Alt+↑/↓` oder am Griff verschieben. `Enter` am Ende legt einen Punkt darunter an,
   * `Rücktaste` im leeren Punkt löscht ihn. Jede Änderung schreibt die ganze Liste (ein Feld).
   */
  import type { ChecklistItemDto } from "@tasks/core-client";
  import { tick, untrack } from "svelte";
  import { GripVertical, ListChecks, X } from "../../icons";

  interface Props {
    items: ChecklistItemDto[];
    onchange: (items: ChecklistItemDto[]) => Promise<void> | void;
  }

  let { items, onchange }: Props = $props();
  const MAX = 100;

  let list = $state<HTMLUListElement>();
  let adding = $state("");
  /** Text in Bearbeitung je Punkt (nur solange fokussiert). */
  let drafts = $state<Record<string, string>>({});
  let drag = $state<{ from: number; to: number } | null>(null);
  /** Neuer, noch leerer Punkt hinter Index `at` (erst mit Text gespeichert). */
  let pending = $state<{ at: number; text: string } | null>(null);

  const done = $derived(items.filter((i) => i.done).length);

  // Mehrere Änderungen kurz hintereinander (Umbenennen + Enter, Verschieben + Verlassen) bauen auf
  // dem zuletzt geschriebenen Stand auf, nicht auf dem noch alten `items`.
  // svelte-ignore state_referenced_locally
  let latest: ChecklistItemDto[] = items;
  $effect(() => {
    latest = items;
    // Entwürfe gelten, bis der gespeicherte Stand da ist (sonst zeigte das Feld kurz den alten Text).
    untrack(() => {
      for (const [id, text] of Object.entries(drafts)) {
        if (items.find((i) => i.id === id)?.text === text.trim()) delete drafts[id];
      }
    });
  });

  function commit(next: ChecklistItemDto[]) {
    latest = next;
    return onchange(next);
  }

  const indexOf = (id: string) => latest.findIndex((item) => item.id === id);

  function toggle(id: string) {
    void commit(latest.map((item) => (item.id === id ? { ...item, done: !item.done } : item)));
  }

  function remove(id: string, focusPrevious = false) {
    const i = indexOf(id);
    if (i < 0) return;
    void commit(latest.filter((item) => item.id !== id));
    if (focusPrevious) void focusAt(Math.max(0, i - 1));
  }

  function rename(id: string, text: string) {
    const value = text.trim();
    const item = latest[indexOf(id)];
    if (!item) return;
    if (!value) return remove(id);
    if (value === item.text) delete drafts[id];
    else drafts[id] = value;
    if (value !== item.text) void commit(latest.map((it) => (it.id === id ? { ...it, text: value } : it)));
  }

  function move(id: string, to: number) {
    const from = indexOf(id);
    if (from < 0 || to < 0 || to >= latest.length || from === to) return;
    const next = [...latest];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    void commit(next);
    void focusAt(to);
  }

  /** Leere Zeile hinter `i` öffnen (Enter am Ende eines Punkts). */
  async function openAfter(i: number) {
    if (latest.length >= MAX) return;
    pending = { at: i, text: "" };
    await tick();
    list?.querySelector<HTMLInputElement>("input.pending")?.focus();
  }

  /** Neue Zeile übernehmen: mit Text einfügen, leer verwerfen; `again` öffnet die nächste. */
  async function commitPending(again: boolean) {
    if (!pending) return;
    const { at, text } = pending;
    pending = null;
    if (!text.trim()) return;
    const next = [...latest];
    next.splice(at + 1, 0, { id: "", text: text.trim(), done: false });
    await commit(next);
    if (again) await openAfter(at + 1);
  }

  async function focusAt(i: number) {
    await tick();
    list?.querySelectorAll<HTMLInputElement>("input.text")[i]?.focus();
  }

  function onkey(e: KeyboardEvent, id: string) {
    const input = e.currentTarget as HTMLInputElement;
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      const text = input.value.trim();
      if (!text) return;
      rename(id, text);
      void openAfter(indexOf(id));
    } else if (e.key === "Backspace" && input.value === "") {
      e.preventDefault();
      remove(id, true);
    } else if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      rename(id, input.value);
      move(id, indexOf(id) + (e.key === "ArrowUp" ? -1 : 1));
    } else if (e.key === "Escape") {
      delete drafts[id];
      input.value = latest[indexOf(id)]?.text ?? "";
      input.blur();
    }
  }

  function addFromField(e: KeyboardEvent) {
    if (e.key !== "Enter" || e.isComposing) return;
    e.preventDefault();
    const text = adding.trim();
    if (!text || latest.length >= MAX) return;
    adding = "";
    void commit([...latest, { id: "", text, done: false }]);
  }

  // Ziehen am Griff: Zielzeile aus der Zeigerposition, Linie davor.
  function startDrag(e: PointerEvent, from: number) {
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { from, to: from };
  }

  function dragMove(e: PointerEvent) {
    if (!drag || !list) return;
    const rows = [...list.querySelectorAll<HTMLElement>("li")];
    const to = rows.findIndex((row) => e.clientY < row.getBoundingClientRect().top + row.offsetHeight / 2);
    drag = { ...drag, to: to < 0 ? rows.length : to };
  }

  function dragEnd() {
    if (!drag) return;
    const { from, to } = drag;
    drag = null;
    const item = latest[from];
    if (item) move(item.id, to > from ? to - 1 : to);
  }
</script>

<div class="checklist">
  {#if items.length > 0}
    <p class="progress" aria-live="polite">
      <ListChecks size={14} strokeWidth={1.75} aria-hidden="true" />{done} von {items.length} erledigt
    </p>
    <ul bind:this={list} aria-label="Checkliste">
      {#each items as item, i (item.id || i)}
        <li class:done={item.done} class:drop-before={drag && drag.to === i && drag.from !== i && drag.from !== i - 1}>
          <span
            class="grip"
            aria-hidden="true"
            onpointerdown={(e) => startDrag(e, i)}
            onpointermove={dragMove}
            onpointerup={dragEnd}
            onpointercancel={() => (drag = null)}
          >
            <GripVertical size={14} strokeWidth={1.75} />
          </span>
          <input type="checkbox" checked={item.done} aria-label="Erledigt: {item.text}" onchange={() => toggle(item.id)} />
          <input
            class="text"
            value={drafts[item.id] ?? item.text}
            aria-label="Punkt {i + 1}"
            maxlength="500"
            oninput={(e) => (drafts[item.id] = e.currentTarget.value)}
            onblur={(e) => rename(item.id, e.currentTarget.value)}
            onkeydown={(e) => onkey(e, item.id)}
          />
          <button type="button" class="remove" aria-label="Punkt löschen: {item.text}" onclick={() => remove(item.id)}>
            <X size={14} strokeWidth={2} aria-hidden="true" />
          </button>
        </li>
        {#if pending?.at === i}
          <li class="new">
            <span class="grip" aria-hidden="true"></span>
            <input type="checkbox" disabled aria-hidden="true" tabindex="-1" />
            <input
              class="text pending"
              aria-label="Neuer Punkt"
              maxlength="500"
              bind:value={pending.text}
              onblur={() => void commitPending(false)}
              onkeydown={(e) => {
                if (e.key === "Enter" && !e.isComposing) {
                  e.preventDefault();
                  void commitPending(true);
                } else if (e.key === "Escape" || (e.key === "Backspace" && pending?.text === "")) {
                  e.preventDefault();
                  pending = null;
                  void focusAt(i);
                }
              }}
            />
          </li>
        {/if}
      {/each}
      {#if drag && drag.to === items.length && drag.from !== items.length - 1}<li class="drop-end" aria-hidden="true"></li>{/if}
    </ul>
  {/if}
  {#if items.length < MAX}
    <input
      class="add"
      placeholder="Punkt hinzufügen …"
      aria-label="Punkt hinzufügen"
      maxlength="500"
      bind:value={adding}
      onkeydown={addFromField}
    />
  {/if}
</div>

<style>
  .checklist {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .progress {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0 0 var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 32px;
    border-radius: var(--radius-md);
  }
  li:hover {
    background: var(--color-bg-hover);
  }
  li.drop-before::before,
  .drop-end::before {
    content: "";
    position: absolute;
    inset: -1px 0 auto;
    height: 2px;
    background: var(--color-accent);
  }
  .drop-end {
    position: relative;
    height: 0;
  }
  .grip {
    display: grid;
    place-items: center;
    width: 16px;
    color: var(--color-text-muted);
    cursor: grab;
    opacity: 0;
    touch-action: none;
  }
  li:hover .grip,
  li:focus-within .grip {
    opacity: 1;
  }
  input[type="checkbox"] {
    flex: none;
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--color-accent-fill);
  }
  .text,
  .add {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
  }
  .text:focus,
  .add:focus {
    outline: none;
    border-color: var(--color-accent);
    background: var(--color-bg);
  }
  li.done .text {
    color: var(--color-text-muted);
    text-decoration: line-through;
  }
  .add {
    margin-left: calc(16px + 16px + 2 * var(--space-2));
    width: auto;
  }
  .add::placeholder {
    color: var(--color-text-muted);
  }
  .remove {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0;
  }
  li:hover .remove,
  li:focus-within .remove {
    opacity: 1;
  }
  .remove:hover {
    color: var(--color-danger);
  }
  @media (pointer: coarse) {
    .grip,
    .remove {
      opacity: 1;
    }
  }
</style>
