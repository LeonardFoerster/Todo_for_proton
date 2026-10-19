<script lang="ts">
  import ListPicker from "../lists/ListPicker.svelte";
  import { CoreClientError, type DueDto, type PriorityDto, type TaskDto, type TaskPatchDto } from "@tasks/core-client";
  import { onMount } from "svelte";
  import Checkbox from "../../components/Checkbox.svelte";
  import IconButton from "../../components/IconButton.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { CalendarDays, Flag, Hash, Trash, X } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import { dueFromInputs, dueLabel, dueToInputDate, dueToInputTime, keepTime, todayPlus } from "../../lib/format-date";
  import { isoDate } from "../../lib/local-day";
  import type { ListsState } from "../../lib/lists.svelte";
  import { listName } from "../../lib/views";

  interface Props {
    taskId: string;
    lists: ListsState;
    onclose: () => void;
    ondelete: (id: string) => void;
  }

  let { taskId, lists, onclose, ondelete }: Props = $props();

  const client = useCoreClient();
  const SAVE_DELAY_MS = 600;

  let task = $state<TaskDto | null>(null);
  let title = $state("");
  let notes = $state("");
  let editing = $state<"title" | "notes" | null>(null);
  let titleInvalid = $state(false);
  let timers: Partial<Record<"title" | "notes", ReturnType<typeof setTimeout>>> = {};

  const due = $derived(task?.due ? dueLabel(task.due) : null);
  const priorities: { value: PriorityDto; label: string }[] = [
    { value: "none", label: "Keine" },
    { value: "low", label: "Niedrig" },
    { value: "medium", label: "Mittel" },
    { value: "high", label: "Hoch" },
  ];
  const DATE = new Intl.DateTimeFormat("de-DE", { dateStyle: "medium", timeStyle: "short" });

  /** Übernimmt den Stand aus dem Core; Felder in Bearbeitung bleiben unangetastet. */
  function adopt(next: TaskDto) {
    task = next;
    if (editing !== "title") title = next.title;
    if (editing !== "notes") notes = next.notes;
  }

  async function load(id: string) {
    flush();
    editing = null;
    try {
      adopt(await client.getTask(id));
    } catch (e) {
      if (CoreClientError.from(e).kind === "notFound") onclose();
    }
  }

  $effect(() => {
    void load(taskId);
  });

  onMount(() => {
    const off = client.onChange((e) => {
      if (e.tasks.includes(taskId)) void load(taskId);
    });
    return () => {
      flush();
      off();
    };
  });

  async function save(patch: TaskPatchDto) {
    if (!task) return;
    try {
      adopt(await client.updateTask(task.id, patch));
    } catch (e) {
      toasts.show({ message: `Nicht gespeichert: ${(e as Error).message}`, tone: "danger" });
    }
  }

  function saveTitle() {
    clearTimeout(timers.title);
    if (!task) return;
    const value = title.trim();
    titleInvalid = !value;
    if (!value || value === task.title) return;
    void save({ title: value });
  }

  function saveNotes() {
    clearTimeout(timers.notes);
    if (task && notes !== task.notes) void save({ notes });
  }

  /** Ausstehende Eingaben sofort speichern (Wechsel der Task, Schließen). */
  function flush() {
    if (timers.title) saveTitle();
    if (timers.notes) saveNotes();
    timers = {};
  }

  function schedule(field: "title" | "notes") {
    clearTimeout(timers[field]);
    timers[field] = setTimeout(field === "title" ? saveTitle : saveNotes, SAVE_DELAY_MS);
  }

  function setDue(next: DueDto | null) {
    void save({ due: next });
  }

  /** Neuer Tag (Feld oder Schnellwahl); eine gesetzte Uhrzeit bleibt. */
  function setDay(next: DueDto | null) {
    setDue(keepTime(next, task?.due ?? null));
  }

  /** Uhrzeit setzen oder leeren (→ ganztägig); ohne Datum gilt heute. */
  function setTime(time: string) {
    const date = dueToInputDate(task?.due ?? null) || isoDate(new Date());
    if (!time && !task?.due) return;
    setDue(dueFromInputs(date, time, task?.due?.kind === "dateTime" ? task.due.tz : undefined));
  }

  async function moveTo(listId: string) {
    if (!task || listId === task.listId) return;
    try {
      adopt(await client.moveTask(task.id, { listId }));
    } catch (e) {
      toasts.show({ message: `Nicht verschoben: ${(e as Error).message}`, tone: "danger" });
    }
  }

  async function toggle(done: boolean) {
    if (!task) return;
    try {
      adopt(await client.completeTask(task.id, done));
    } catch (e) {
      toasts.show({ message: `Nicht gespeichert: ${(e as Error).message}`, tone: "danger" });
    }
  }
</script>

<section class="detail" aria-label="Task-Details">
  {#if task}
    <header>
      <Checkbox
        checked={task.status === "done"}
        label={task.status === "done" ? "Wieder öffnen" : "Erledigt"}
        tone={task.priority === "none" || task.status === "done" ? "default" : task.priority}
        onchange={(v) => void toggle(v)}
      />
      <span class="where">
        <Hash size={14} strokeWidth={1.75} aria-hidden="true" />
        {listName(lists.lists.find((l) => l.id === task!.listId) ?? { id: "", name: "Liste", isInbox: false, sortKey: "" })}
      </span>
      <IconButton label="Löschen" shortcut="Entf" icon={Trash} size={28} onclick={() => ondelete(task!.id)} />
      <IconButton label="Schließen" shortcut="Esc" icon={X} size={28} onclick={onclose} />
    </header>

    <div class="body">
      <textarea
        class="title"
        class:invalid={titleInvalid}
        rows="1"
        maxlength="500"
        aria-label="Titel"
        aria-invalid={titleInvalid || undefined}
        bind:value={title}
        onfocus={() => (editing = "title")}
        onblur={() => {
          editing = null;
          saveTitle();
          if (!title.trim() && task) {
            title = task.title;
            titleInvalid = false;
          }
        }}
        oninput={() => schedule("title")}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            (e.currentTarget as HTMLTextAreaElement).blur();
          }
        }}
      ></textarea>
      {#if titleInvalid}<p class="error" role="alert">Der Titel darf nicht leer sein.</p>{/if}

      <textarea
        class="notes"
        rows="5"
        placeholder="Notizen"
        aria-label="Notizen"
        bind:value={notes}
        onfocus={() => (editing = "notes")}
        onblur={() => {
          editing = null;
          saveNotes();
        }}
        oninput={() => schedule("notes")}
      ></textarea>

      <dl class="props">
        <dt><CalendarDays size={16} strokeWidth={1.75} aria-hidden="true" /><label for="detail-due">Fällig</label></dt>
        <dd>
          <div class="due-row">
            <input
              id="detail-due"
              type="date"
              class="field date {due?.tone ?? ''}"
              value={dueToInputDate(task.due)}
              onchange={(e) => {
                const v = (e.currentTarget as HTMLInputElement).value;
                setDay(v ? { kind: "date", date: v } : null);
              }}
            />
            <input
              type="time"
              class="field time"
              aria-label="Uhrzeit"
              value={dueToInputTime(task.due)}
              onchange={(e) => setTime((e.currentTarget as HTMLInputElement).value)}
            />
            {#if due}<span class="due-text {due.tone}">{due.text}</span>{/if}
          </div>
          <div class="chips">
            <button type="button" class="chip" onclick={() => setDay(todayPlus(0))}>Heute</button>
            <button type="button" class="chip" onclick={() => setDay(todayPlus(1))}>Morgen</button>
            <button type="button" class="chip" onclick={() => setDay(todayPlus(7))}>In einer Woche</button>
            {#if task.due}<button type="button" class="chip ghost" onclick={() => setDue(null)}>Entfernen</button>{/if}
          </div>
        </dd>

        <dt><Flag size={16} strokeWidth={1.75} aria-hidden="true" /><span id="detail-prio">Priorität</span></dt>
        <dd>
          <div class="segments" role="radiogroup" aria-labelledby="detail-prio">
            {#each priorities as p (p.value)}
              <button
                type="button"
                role="radio"
                class="segment {p.value}"
                aria-checked={task.priority === p.value}
                onclick={() => void save({ priority: p.value })}
              >
                {#if p.value !== "none"}<Flag size={12} strokeWidth={2} aria-hidden="true" />{/if}{p.label}
              </button>
            {/each}
          </div>
        </dd>

        <dt><Hash size={16} strokeWidth={1.75} aria-hidden="true" /><label for="detail-list">Liste</label></dt>
        <dd>
          <ListPicker id="detail-list" {lists} value={task.listId} onchange={(id) => void moveTo(id)} />
        </dd>
      </dl>
    </div>

    <footer>
      Erstellt {DATE.format(new Date(task.createdAt))} · Geändert {DATE.format(new Date(task.updatedAt))}
    </footer>
  {/if}
</section>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--color-bg);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 56px;
    padding: 0 var(--space-3) 0 var(--space-5);
    border-bottom: 1px solid var(--color-border);
    flex: none;
  }
  .where {
    display: inline-flex;
    flex: 1;
    min-width: 0;
    align-items: center;
    gap: 4px;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-4) var(--space-5);
  }
  textarea {
    display: block;
    width: 100%;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    resize: none;
    transition:
      border-color var(--duration-fast) var(--ease),
      background-color var(--duration-fast) var(--ease);
  }
  textarea:hover {
    border-color: var(--color-border);
  }
  textarea:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
  .title {
    field-sizing: content;
    min-height: 40px;
    margin: 0 calc(-1 * var(--space-2));
    width: calc(100% + 2 * var(--space-2));
    padding: 6px var(--space-2);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    line-height: 1.35;
  }
  .title.invalid {
    border-color: var(--color-danger);
  }
  .error {
    margin: var(--space-1) 0 0;
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
  .notes {
    margin: var(--space-3) calc(-1 * var(--space-2)) 0;
    width: calc(100% + 2 * var(--space-2));
    min-height: 96px;
    padding: var(--space-2);
    line-height: 1.55;
    resize: vertical;
  }
  .notes::placeholder {
    color: var(--color-text-muted);
  }
  .props {
    margin: var(--space-6) 0 0;
    padding-top: var(--space-2);
    border-top: 1px solid var(--color-border);
  }
  dt {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-4) 0 var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  dd {
    margin: 0;
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
  .due-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .due-text {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .due-text.overdue {
    color: var(--color-danger);
  }
  .due-text.today {
    color: var(--color-accent-text);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-2);
  }
  .chip {
    height: 26px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .chip:hover {
    background: var(--color-bg-hover);
  }
  .chip.ghost {
    border-color: transparent;
    color: var(--color-text-muted);
  }
  .segments {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-md);
    background: var(--color-bg-hover);
  }
  .segment {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 28px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .segment:hover {
    color: var(--color-text);
  }
  .segment[aria-checked="true"] {
    background: var(--color-bg);
    color: var(--color-text);
    font-weight: var(--weight-medium);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
  }
  .segment.high :global(svg) {
    color: var(--color-danger);
  }
  .segment.medium :global(svg) {
    color: var(--color-warning);
  }
  .segment.low :global(svg) {
    color: var(--color-accent);
  }
  footer {
    flex: none;
    padding: var(--space-3) var(--space-5);
    border-top: 1px solid var(--color-border);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }
</style>
