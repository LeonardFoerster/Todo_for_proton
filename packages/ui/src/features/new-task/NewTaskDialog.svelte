<script lang="ts">
  /**
   * Fenster „Neue Task“ (P3-03, geöffnet über „+ Neue Task“, `N`, `Strg+N`). Schneller Weg bleibt die
   * Titelzeile: Fälligkeit, Uhrzeit, Priorität und Liste werden aus dem Text erkannt (`parse.ts`) und als
   * Chips gezeigt. Ein Chip, der von Hand gesetzt wurde, gilt vor dem Text. Optional eine Notiz.
   * `Enter` (Titel) bzw. `Strg+Enter` (Notiz) legt an – in einem Aufruf – und schließt, außer
   * „Weitere anlegen“ ist an. `Esc` schließt.
   */
  import type { DueDto, PriorityDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import Textarea from "../../components/Textarea.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { CalendarDays, Check, Flag, Pencil, Plus, X } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import { dueFromInputs, dueLabel, dueToInputDate, dueToInputTime, keepTime, todayPlus } from "../../lib/format-date";
  import type { ListsState } from "../../lib/lists.svelte";
  import { readFlag, writeFlag } from "../../lib/preferences";
  import ListPicker from "../lists/ListPicker.svelte";
  import { parseNewTask } from "./parse";

  interface Props {
    lists: ListsState;
    /** Vorausgewählte Zielliste (`undefined` = Eingang). */
    listId?: string;
    /**
     * Liste, deren Tasks die Ansicht zeigt (Liste/Eingang); `undefined` in Sammelansichten. Landet
     * eine neue Task woanders, sagt ein Toast wo (mit „Öffnen“).
     */
    shownListId?: string;
    onopenlist?: (listId: string) => void;
    /** Vorgabe ohne Angabe im Text (z. B. „heute“ in der Heute-Ansicht). */
    defaultDue?: DueDto | null;
    /** Vorbelegter Text der Titelzeile (z. B. aus der Befehlspalette ohne Treffer). */
    initialText?: string;
    onclose: () => void;
  }

  let { lists, listId, shownListId, onopenlist, defaultDue = null, initialText = "", onclose }: Props = $props();

  const KEEP_OPEN_KEY = "tasks.new-task-keep-open";
  const PRIORITIES: { value: PriorityDto; label: string }[] = [
    { value: "none", label: "Keine" },
    { value: "low", label: "Niedrig" },
    { value: "medium", label: "Mittel" },
    { value: "high", label: "Hoch" },
  ];

  const client = useCoreClient();
  let input = $state<HTMLInputElement>();
  let notesField = $state<HTMLTextAreaElement>();
  let picker = $state<ListPicker>();
  let prioButton = $state<HTMLButtonElement>();
  let busy = false;

  // svelte-ignore state_referenced_locally
  let value = $state(initialText);
  let notes = $state("");
  let notesOpen = $state(false);
  let dueOpen = $state(false);
  let prioMenu = $state<{ x: number; y: number } | null>(null);
  let keepOpen = $state(readFlag(KEEP_OPEN_KEY));
  // Von Hand gesetzt (`undefined` = aus dem Text bzw. Vorgabe); `null` heißt „bewusst keine“.
  let manualDue = $state<DueDto | null | undefined>();
  let manualPriority = $state<PriorityDto | undefined>();
  let chosenList = $state<string | undefined>();

  const parsed = $derived(parseNewTask(value, { lists: lists.lists }));
  const due = $derived(manualDue !== undefined ? manualDue : (parsed.due ?? defaultDue));
  const priority = $derived(manualPriority ?? parsed.priority ?? "none");
  const target = $derived(chosenList ?? parsed.listId ?? listId);
  const dueText = $derived(due ? dueLabel(due) : null);
  const prioLabel = $derived(PRIORITIES.find((p) => p.value === priority)!.label);
  /** Titel weicht vom Eingetippten ab → zeigen, was angelegt wird. */
  const titlePreview = $derived(parsed.title && parsed.title !== value.trim() ? parsed.title : null);

  onMount(() => {
    void tick().then(() => input?.focus());
  });

  /** `Alt+L`: Listenwahl öffnen. */
  export function openListPicker() {
    picker?.open();
  }

  /** `Alt+D`: Fälligkeit auf-/zuklappen. */
  export function toggleDue() {
    dueOpen = !dueOpen;
  }

  /** `Alt+P`: Prioritätsmenü öffnen. */
  export function openPriority() {
    const r = prioButton?.getBoundingClientRect();
    prioMenu = { x: r?.left ?? 0, y: (r?.bottom ?? 0) + 4 };
  }

  /** `Alt+N`: Notiz auf-/zuklappen (aufgeklappt mit Fokus). */
  export function toggleNotes() {
    notesOpen = !notesOpen;
    if (notesOpen) void tick().then(() => notesField?.focus());
    else input?.focus();
  }

  function prioItems(): MenuItem[] {
    return PRIORITIES.map((p) => ({
      id: p.value,
      label: p.label,
      icon: p.value === priority ? Check : Flag,
      action: () => {
        manualPriority = p.value;
        input?.focus();
      },
    }));
  }

  function chooseList(id: string) {
    chosenList = id;
    input?.focus();
  }

  function setDue(next: DueDto | null) {
    manualDue = next;
  }

  /** Schnellwahl: setzen und zurück in den Titel (der Button kann dabei verschwinden). */
  function pickDue(next: DueDto | null) {
    setDue(keepTime(next, due));
    input?.focus();
  }

  /** Neue Task liegt nicht in der angezeigten Liste: sagen, wo, mit Sprung dorthin. */
  function announce(title: string, targetId: string) {
    if (shownListId === undefined || targetId === shownListId) return;
    const list = lists.lists.find((l) => l.id === targetId);
    const name = list ? (list.isInbox ? "Eingang" : list.name) : "Liste";
    toasts.show({
      message: `„${title.length > 40 ? `${title.slice(0, 39)}…` : title}“ in „${name}“ angelegt`,
      actionLabel: onopenlist ? "Öffnen" : undefined,
      onAction: onopenlist ? () => onopenlist(targetId) : undefined,
    });
  }

  /** Nach dem Anlegen bei „Weitere anlegen“: alles außer der Listenwahl zurücksetzen. */
  function reset() {
    value = "";
    notes = "";
    notesOpen = false;
    dueOpen = false;
    manualDue = undefined;
    manualPriority = undefined;
  }

  async function submit() {
    const { title } = parsed;
    if (!title || busy) return;
    busy = true;
    try {
      const task = await client.createTask({
        title,
        listId: target,
        notes: notes.trim() ? notes : undefined,
        priority: priority === "none" ? undefined : priority,
        due: due ?? undefined,
      });
      announce(title, task.listId);
      if (keepOpen) reset();
      else onclose();
    } catch (e) {
      toasts.show({ message: `Nicht angelegt: ${(e as Error).message}`, tone: "danger" });
    } finally {
      busy = false;
      input?.focus();
    }
  }
</script>

<Dialog title="Neue Task" {onclose} width={560}>
  <div class="body" data-new-task>
    <div class="title-row">
      <span class="plus" aria-hidden="true"><Plus size={18} strokeWidth={2} /></span>
      <input
        bind:this={input}
        bind:value
        placeholder="Titel – z. B. „Übungsblatt 3 bis Fr 14:00 !1“"
        aria-label="Neue Task"
        aria-describedby="new-task-hint"
        maxlength="500"
        autocomplete="off"
        spellcheck="true"
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.isComposing) {
            e.preventDefault();
            void submit();
          } else if (e.key === "Escape") {
            // Selbst schließen: ohne `showModal` (jsdom) gibt es kein `cancel`.
            e.preventDefault();
            onclose();
          }
        }}
      />
    </div>
    <p class="hint" id="new-task-hint">
      {#if titlePreview}
        Titel: <strong>{titlePreview}</strong>
      {:else}
        <code>morgen</code> · <code>Fr</code> · <code>24.10.</code> · <code>14:00</code> · <code>!1</code>–<code>!3</code>
        · <code>@liste</code>
      {/if}
    </p>

    <div class="chips">
      <button
        type="button"
        class="chip {dueText?.tone ?? ''}"
        class:empty={!due}
        aria-expanded={dueOpen}
        aria-controls="new-task-due"
        data-tooltip="Fälligkeit   Alt+D"
        onclick={toggleDue}
      >
        <CalendarDays size={14} strokeWidth={1.75} aria-hidden="true" />{dueText?.text ?? "Fällig"}
      </button>
      <button
        bind:this={prioButton}
        type="button"
        class="chip prio-{priority}"
        class:empty={priority === "none"}
        aria-haspopup="menu"
        aria-expanded={prioMenu !== null}
        aria-label="Priorität: {prioLabel}"
        data-tooltip="Priorität   Alt+P"
        onclick={openPriority}
      >
        <Flag size={14} strokeWidth={1.75} aria-hidden="true" />{priority === "none" ? "Priorität" : prioLabel}
      </button>
      <ListPicker bind:this={picker} {lists} value={target} onchange={chooseList} shortcut="Alt+L" />
      <button
        type="button"
        class="chip"
        class:empty={!notes.trim()}
        aria-expanded={notesOpen}
        aria-controls="new-task-notes"
        data-tooltip="Notiz   Alt+N"
        onclick={toggleNotes}
      >
        <Pencil size={14} strokeWidth={1.75} aria-hidden="true" />Notiz
      </button>
    </div>

    {#if dueOpen}
      <div class="due-panel" id="new-task-due">
        <input
          type="date"
          class="field"
          aria-label="Fällig am"
          value={dueToInputDate(due)}
          onchange={(e) => setDue(dueFromInputs(e.currentTarget.value, dueToInputTime(due)))}
        />
        <input
          type="time"
          class="field"
          aria-label="Uhrzeit"
          disabled={!due}
          value={dueToInputTime(due)}
          onchange={(e) => setDue(dueFromInputs(dueToInputDate(due), e.currentTarget.value))}
        />
        <button type="button" class="chip" onclick={() => pickDue(todayPlus(0))}>Heute</button>
        <button type="button" class="chip" onclick={() => pickDue(todayPlus(1))}>Morgen</button>
        <button type="button" class="chip" onclick={() => pickDue(todayPlus(7))}>In einer Woche</button>
        {#if due}
          <button type="button" class="chip ghost" onclick={() => pickDue(null)}>
            <X size={14} strokeWidth={1.75} aria-hidden="true" />Entfernen
          </button>
        {/if}
      </div>
    {/if}

    {#if notesOpen}
      <div id="new-task-notes">
        <Textarea
          bind:ref={notesField}
          bind:value={notes}
          rows={4}
          aria-label="Notiz"
          placeholder="Notiz (optional)"
          maxlength={20000}
          onkeydown={(e) => {
            if (e.key === "Enter" && e.ctrlKey) {
              e.preventDefault();
              void submit();
            } else if (e.key === "Escape") {
              e.preventDefault();
              onclose();
            }
          }}
        />
      </div>
    {/if}

    <footer>
      <label class="keep">
        <input
          type="checkbox"
          bind:checked={keepOpen}
          onchange={() => {
            writeFlag(KEEP_OPEN_KEY, keepOpen);
            input?.focus();
          }}
        />
        Weitere anlegen
      </label>
      <Button variant="primary" disabled={!parsed.title} onclick={() => void submit()}>Anlegen</Button>
    </footer>
  </div>
</Dialog>

{#if prioMenu}
  <Menu label="Priorität wählen" items={prioItems()} x={prioMenu.x} y={prioMenu.y} onclose={() => (prioMenu = null)} />
{/if}

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 44px;
    padding: 0 var(--space-3) 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    transition:
      border-color var(--duration-fast) var(--ease),
      box-shadow var(--duration-fast) var(--ease);
  }
  .title-row:hover {
    border-color: var(--color-border-strong);
  }
  .title-row:focus-within {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
  .plus {
    display: inline-flex;
    color: var(--color-accent);
  }
  .title-row input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-md);
  }
  .title-row input:focus {
    outline: none;
  }
  .title-row input::placeholder {
    color: var(--color-text-muted);
  }
  .hint {
    min-height: 1lh;
    margin: calc(-1 * var(--space-2)) 0 0 var(--space-2);
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .hint strong {
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }
  .hint code {
    font-family: inherit;
    padding: 0 3px;
    border-radius: 4px;
    background: var(--color-bg-hover);
  }
  .chips,
  .due-panel {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease),
      color var(--duration-fast) var(--ease);
  }
  .chip:hover,
  .chip[aria-expanded="true"] {
    background: var(--color-bg-hover);
  }
  .chip:focus-visible {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: 1px;
  }
  .chip.empty {
    color: var(--color-text-muted);
  }
  .chip.ghost {
    border-color: transparent;
    color: var(--color-text-muted);
  }
  .chip.overdue {
    color: var(--color-danger);
  }
  .chip.today {
    color: var(--color-accent);
  }
  .chip.prio-high :global(svg) {
    color: var(--color-danger);
  }
  .chip.prio-medium :global(svg) {
    color: var(--color-warning);
  }
  .chip.prio-low :global(svg) {
    color: var(--color-accent);
  }
  .field {
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }
  .field:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
  .field:disabled {
    opacity: 0.5;
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px solid var(--color-border);
  }
  .keep {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .keep input {
    accent-color: var(--color-accent);
  }
</style>
