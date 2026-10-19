<script lang="ts" module>
  /** Befehl der Palette – kommt aus der Kürzel-Registry (`palette`), ausgeführt ohne Tastaturereignis. */
  export interface PaletteCommand {
    id: string;
    label: string;
    shortcut?: string;
    run: () => void;
  }

  export interface RecentTask {
    id: string;
    title: string;
  }
</script>

<script lang="ts">
  /**
   * Befehlspalette (P4-05): `Strg+K` bzw. `/`. Tippen sucht Tasks in allen Listen (Core,
   * entprellt, höchstens 50), dazu passende Listen und Befehle. ↑/↓ wählen, Enter führt aus, Esc
   * schließt. Ohne Text: zuletzt geöffnete Tasks dieser Sitzung und alle Befehle. Ohne Treffer legt
   * Enter eine neue Task mit dem Text an. Wird erst beim ersten Öffnen geladen (lazy).
   */
  import type { TaskListDto, TaskSummaryDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { Circle, CircleCheck, Clock, Hash, Inbox, Search, Zap } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import { dueLabel } from "../../lib/format-date";
  import type { ListsState } from "../../lib/lists.svelte";
  import { listName } from "../../lib/views";
  import { highlight, matchesAll, terms } from "./palette";

  interface Props {
    lists: ListsState;
    commands: readonly PaletteCommand[];
    /** Zuletzt geöffnete Tasks (neueste zuerst, nur im Speicher der Sitzung). */
    recent: readonly RecentTask[];
    onopentask: (id: string) => void;
    onopenlist: (list: TaskListDto) => void;
    /** Kein Treffer: „Neue Task“ mit diesem Text öffnen. */
    onnewtask: (text: string) => void;
    onclose: () => void;
  }

  let { lists, commands, recent, onopentask, onopenlist, onnewtask, onclose }: Props = $props();

  const client = useCoreClient();
  const LIMIT = 50;
  const DEBOUNCE_MS = 100;
  const uid = $props.id();

  type Item =
    | { kind: "task"; task: TaskSummaryDto }
    | { kind: "recent"; task: RecentTask }
    | { kind: "list"; list: TaskListDto }
    | { kind: "command"; command: PaletteCommand };

  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let text = $state("");
  /** Tasks zum Text `searched` (nach dem Entprellen); `null` = noch keine Antwort. */
  let results = $state.raw<TaskSummaryDto[] | null>(null);
  let searched = $state("");
  let active = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;

  const words = $derived(terms(text));
  const names = $derived(new Map(lists.lists.map((l) => [l.id, listName(l)])));

  const sections = $derived.by(() => {
    const out: { label: string; items: Item[] }[] = [];
    if (words.length === 0) {
      if (recent.length > 0) out.push({ label: "Zuletzt geöffnet", items: recent.map((task) => ({ kind: "recent", task })) });
    } else if (results && results.length > 0) {
      out.push({ label: "Tasks", items: results.map((task) => ({ kind: "task", task })) });
    }
    const matchingLists = lists.lists.filter((l) => words.length > 0 && matchesAll(listName(l), words)).slice(0, 5);
    if (matchingLists.length > 0) out.push({ label: "Listen", items: matchingLists.map((list) => ({ kind: "list", list })) });
    const matchingCommands = commands.filter((c) => matchesAll(c.label, words)).slice(0, 8);
    if (matchingCommands.length > 0) {
      out.push({ label: "Befehle", items: matchingCommands.map((command) => ({ kind: "command", command })) });
    }
    return out;
  });
  const flat = $derived(sections.flatMap((s) => s.items));
  /** Suche für den aktuellen Text ist beantwortet und nichts passt. */
  const nothing = $derived(words.length > 0 && searched === text && flat.length === 0);

  onMount(() => {
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    input.focus();
    return () => {
      clearTimeout(timer);
      if (typeof dialog.close === "function") dialog.close();
    };
  });

  function oninput(e: Event) {
    active = 0;
    clearTimeout(timer);
    // Aus dem Feld lesen: `bind:value` kann `text` erst nach diesem Handler setzen.
    const value = (e.currentTarget as HTMLInputElement).value;
    if (!value.trim()) {
      results = null;
      searched = value;
      return;
    }
    timer = setTimeout(() => void search(value), DEBOUNCE_MS);
  }

  async function search(value: string) {
    const mine = ++request;
    try {
      const found = await client.searchTasks(value, LIMIT);
      if (mine !== request) return; // überholt
      results = found;
      searched = value;
      active = 0;
    } catch (e) {
      if (mine === request) toasts.show({ message: `Suche fehlgeschlagen: ${(e as Error).message}`, tone: "danger" });
    }
  }

  function run(item: Item) {
    onclose();
    if (item.kind === "task" || item.kind === "recent") onopentask(item.task.id);
    else if (item.kind === "list") onopenlist(item.list);
    else item.command.run();
  }

  async function move(delta: 1 | -1) {
    if (flat.length === 0) return;
    active = (active + delta + flat.length) % flat.length;
    await tick();
    document.getElementById(`${uid}-opt-${active}`)?.scrollIntoView({ block: "nearest" });
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      void move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      void move(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = flat[active];
      if (item) run(item);
      else if (words.length > 0 && searched === text) {
        onclose();
        onnewtask(text.trim());
      }
    }
  }

  /** Laufende Nummer eines Eintrags über alle Abschnitte (für `aria-activedescendant`). */
  function indexOf(item: Item): number {
    return flat.indexOf(item);
  }
</script>

<dialog
  bind:this={dialog}
  class="palette"
  aria-label="Suchen und Befehle"
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
  onclick={(e) => {
    if (e.target === dialog) onclose();
  }}
>
  <div class="field">
    <Search size={18} strokeWidth={1.75} aria-hidden="true" />
    <input
      bind:this={input}
      bind:value={text}
      {oninput}
      {onkeydown}
      role="combobox"
      aria-label="Suchen"
      aria-expanded={flat.length > 0}
      aria-controls="{uid}-results"
      aria-autocomplete="list"
      aria-activedescendant={flat.length > 0 ? `${uid}-opt-${active}` : undefined}
      placeholder="Tasks suchen oder Befehl eingeben…"
      autocomplete="off"
      spellcheck="false"
    />
  </div>

  <div class="results" id="{uid}-results" role="listbox" aria-label="Ergebnisse">
    {#each sections as section (section.label)}
      <div role="group" aria-labelledby="{uid}-{section.label}">
        <div class="section" id="{uid}-{section.label}" role="presentation">{section.label}</div>
        {#each section.items as item (item.kind === "command" ? `c:${item.command.id}` : item.kind === "list" ? `l:${item.list.id}` : `t:${item.task.id}`)}
          {@const i = indexOf(item)}
          <!-- Tastatur läuft über das Eingabefeld (aria-activedescendant); Klick wählt direkt. -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            id="{uid}-opt-{i}"
            class="item"
            class:active={i === active}
            role="option"
            aria-selected={i === active}
            tabindex="-1"
            onclick={() => run(item)}
            onpointermove={() => (active = i)}
          >
            {#if item.kind === "task"}
              {@const t = item.task}
              {@const due = t.due ? dueLabel(t.due) : null}
              <span class="icon" class:done={t.status === "done"}>
                {#if t.status === "done"}<CircleCheck size={16} strokeWidth={1.75} aria-hidden="true" />{:else}<Circle
                    size={16}
                    strokeWidth={1.75}
                    aria-hidden="true"
                  />{/if}
              </span>
              <span class="label" class:done={t.status === "done"}>
                {#each highlight(t.title, words) as seg, s (s)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}
              </span>
              <span class="meta">
                {#if !words.some((w) => t.title.toLowerCase().includes(w))}<span class="in-notes">in Notiz</span>{/if}
                {#if due}<span class="due {due.tone}">{due.text}</span>{/if}
                <span class="list"><Hash size={12} strokeWidth={2} aria-hidden="true" />{names.get(t.listId) ?? "Liste"}</span>
              </span>
            {:else if item.kind === "recent"}
              <span class="icon"><Clock size={16} strokeWidth={1.75} aria-hidden="true" /></span>
              <span class="label">{item.task.title}</span>
            {:else if item.kind === "list"}
              <span class="icon">
                {#if item.list.isInbox}<Inbox size={16} strokeWidth={1.75} aria-hidden="true" />{:else}<Hash
                    size={16}
                    strokeWidth={1.75}
                    aria-hidden="true"
                  />{/if}
              </span>
              <span class="label">
                {#each highlight(listName(item.list), words) as seg, s (s)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}
              </span>
            {:else}
              <span class="icon"><Zap size={16} strokeWidth={1.75} aria-hidden="true" /></span>
              <span class="label">{item.command.label}</span>
              {#if item.command.shortcut}<kbd>{item.command.shortcut}</kbd>{/if}
            {/if}
          </div>
        {/each}
      </div>
    {/each}
  </div>

  {#if nothing}
    <p class="empty" role="status">Keine Task mit „{text.trim()}“ – mit Enter als neue Task anlegen</p>
  {/if}
  <p class="hints" aria-hidden="true"><kbd>↑</kbd><kbd>↓</kbd> wählen · <kbd>Enter</kbd> öffnen · <kbd>Esc</kbd> schließen</p>
</dialog>

<style>
  .palette {
    width: min(560px, calc(100vw - 32px));
    margin: 12vh auto auto;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg-raised);
    color: var(--color-text);
    box-shadow: var(--shadow-popover);
    animation: palette-in var(--duration-fast) var(--ease);
  }
  .palette:focus {
    outline: none;
  }
  .palette::backdrop {
    background: rgb(0 0 0 / 0.25);
  }
  @keyframes palette-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  .field {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-muted);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 48px;
    border: 0;
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-lg);
  }
  input:focus {
    outline: none;
  }
  .palette:has(input:focus-visible) .field {
    box-shadow: inset 0 -2px 0 var(--color-focus-outline);
  }
  input::placeholder {
    color: var(--color-text-muted);
  }
  .results {
    max-height: min(400px, 56vh);
    overflow-y: auto;
    padding: var(--space-1) 0;
  }
  .results:empty {
    display: none;
  }
  .section {
    padding: var(--space-2) var(--space-4) var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--row-height);
    margin: 0 var(--space-2);
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    cursor: pointer;
  }
  .item.active {
    background: var(--color-bg-selected);
  }
  .icon {
    display: grid;
    place-items: center;
    flex: none;
    color: var(--color-text-muted);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .label.done {
    color: var(--color-text-muted);
    text-decoration: line-through;
  }
  mark {
    background: none;
    color: var(--color-accent-text);
    font-weight: var(--weight-semibold);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex: none;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .list {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    max-width: 140px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .in-notes {
    font-style: italic;
  }
  .due.overdue {
    color: var(--color-danger);
  }
  .due.today {
    color: var(--color-accent-text);
  }
  kbd {
    padding: 0 6px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-xs);
  }
  .empty {
    margin: 0;
    padding: var(--space-4);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .hints {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 0;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--color-border);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }
  @media (prefers-reduced-motion: reduce) {
    .palette {
      animation: none;
    }
  }
</style>
