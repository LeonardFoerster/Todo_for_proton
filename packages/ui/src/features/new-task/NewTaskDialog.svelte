<script lang="ts">
  /**
   * Fenster „Neue Task“ (P3-03, als Schnelleingabe überarbeitet in P6; geöffnet über „+ Neue Task“, `N`,
   * `Strg+N`). Oben verankert, ohne Kopf. Die Titelzeile erkennt Fälligkeit, Uhrzeit, Priorität und Liste
   * (`parse.ts`) und markiert die Stellen im Text; die Chips darunter zeigen die Werte (gesetzt getönt mit ×,
   * leer nur Kontur). Ein von Hand gesetzter Chip gilt vor dem Text. Darunter randlos die Notiz.
   * `Enter` (Titel) bzw. `Strg+Enter` (Notiz) legt an – in einem Aufruf – und schließt, außer
   * „Weitere anlegen“ ist an (dann bestätigt eine Zeile im Fenster). `Esc` schließt.
   */
  import type { DueDto, PriorityDto } from "@tasks/core-client";
  import { onMount, tick } from "svelte";
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import IconButton from "../../components/IconButton.svelte";
  import Menu, { type MenuItem } from "../../components/Menu.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { CalendarDays, Check, Circle, CircleHelp, X } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import { density } from "../../lib/density.svelte";
  import { dueLabel } from "../../lib/format-date";
  import type { ListsState } from "../../lib/lists.svelte";
  import { parseIsoDate } from "../../lib/local-day";
  import { readFlag, writeFlag } from "../../lib/preferences";
  import DuePopover from "../due/DuePopover.svelte";
  import ListPicker from "../lists/ListPicker.svelte";
  import { parseNewTask, recognizedMarks, type TextMark } from "./parse";

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
  /** Syntax der Titelzeile – im Tooltip des „?“ statt als eigene Zeile (P6-01). */
  const SYNTAX = [
    "Im Titel erkannt:",
    "morgen · Fr · 24.10. · 14:00   Fälligkeit",
    "!1 hoch · !2 mittel · !3 niedrig   Priorität",
    "@Liste   Liste",
  ].join("\n");
  const DAY = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short" });

  const client = useCoreClient();
  let input = $state<HTMLInputElement>();
  let notesField = $state<HTMLTextAreaElement>();
  let picker = $state<ListPicker>();
  let prioButton = $state<HTMLButtonElement>();
  let dueButton = $state<HTMLButtonElement>();
  let busy = false;

  // svelte-ignore state_referenced_locally
  let value = $state(initialText);
  let notes = $state("");
  let dueOpen = $state(false);
  let prioMenu = $state<{ x: number; y: number } | null>(null);
  let keepOpen = $state(readFlag(KEEP_OPEN_KEY));
  /** Bestätigung im Fenster nach dem Anlegen mit „Weitere anlegen“. */
  let created = $state<string | null>(null);
  /** Waagerechter Scroll des Titelfelds – die Markierungsebene läuft mit. */
  let scroll = $state(0);
  // Von Hand gesetzt (`undefined` = aus dem Text bzw. Vorgabe); `null` heißt „bewusst keine“.
  let manualDue = $state<DueDto | null | undefined>();
  let manualPriority = $state<PriorityDto | undefined>();
  let chosenList = $state<string | undefined>();

  const parsed = $derived(parseNewTask(value, { lists: lists.lists }));
  const due = $derived(manualDue !== undefined ? manualDue : (parsed.due ?? defaultDue));
  const priority = $derived(manualPriority ?? parsed.priority ?? "none");
  const target = $derived(chosenList ?? parsed.listId ?? listId);
  const dueTone = $derived(due ? dueLabel(due).tone : null);
  const prioLabel = $derived(PRIORITIES.find((p) => p.value === priority)!.label);

  /** Wirksame Markierungen: ein von Hand gesetzter Chip hebt die seiner Art auf. */
  const marks = $derived(
    recognizedMarks(value, { lists: lists.lists }).filter(
      (m) =>
        (m.kind === "due" && manualDue === undefined) ||
        (m.kind === "priority" && manualPriority === undefined) ||
        (m.kind === "list" && chosenList === undefined),
    ),
  );
  /** Text der Markierungsebene in Stücken; `mark` = erkannte Stelle. */
  const segments = $derived.by(() => {
    const out: { text: string; mark?: TextMark }[] = [];
    let at = 0;
    for (const mark of marks) {
      if (mark.start > at) out.push({ text: value.slice(at, mark.start) });
      out.push({ text: value.slice(mark.start, mark.end), mark });
      at = mark.end;
    }
    if (at < value.length) out.push({ text: value.slice(at) });
    return out;
  });

  onMount(() => {
    void tick().then(() => input?.focus());
  });

  /** Chip-Text der Fälligkeit: „Heute“/„Morgen“ wie in der Zeile, sonst mit Datum („Fr, 23. Okt. 14:00“). */
  function dueChipText(d: DueDto): string {
    const label = dueLabel(d);
    if (/^(Heute|Morgen|Gestern)/.test(label.text)) return label.text;
    const day = d.kind === "date" ? parseIsoDate(d.date) : new Date(d.at);
    const time = d.kind === "dateTime" ? ` ${label.text.split(" ").at(-1)}` : "";
    return `${DAY.format(day).replace(/^(\p{L}+)\./u, "$1")}${time}`;
  }

  /** `Alt+L`: Listenwahl öffnen. */
  export function openListPicker() {
    picker?.open();
  }

  /** `Alt+D`: Fälligkeit (Popover am Chip) auf-/zuklappen. */
  export function toggleDue() {
    dueOpen = !dueOpen;
    if (!dueOpen) input?.focus();
  }

  /** `Alt+P`: Prioritätsmenü öffnen. */
  export function openPriority() {
    const r = prioButton?.getBoundingClientRect();
    prioMenu = { x: r?.left ?? 0, y: (r?.bottom ?? 0) + 4 };
  }

  /** `Alt+N`: in die Notiz springen bzw. zurück in den Titel. */
  export function toggleNotes() {
    if (document.activeElement === notesField) input?.focus();
    else notesField?.focus();
  }

  function prioItems(): MenuItem[] {
    return PRIORITIES.map((p) => ({
      id: p.value,
      label: p.label,
      icon: p.value === priority ? Check : Circle,
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

  /** Wahl im Datums-Popover (Uhrzeit bleibt dort): setzen, schließen, zurück in den Titel. */
  function pickDue(next: DueDto | null) {
    manualDue = next;
    dueOpen = false;
    input?.focus();
  }

  function listLabel(id: string | undefined): string {
    const list = lists.lists.find((l) => (id === undefined ? l.isInbox : l.id === id));
    return list ? (list.isInbox ? "Eingang" : list.name) : "Liste";
  }

  /**
   * Nach dem Anlegen: bleibt das Fenster offen, bestätigt eine Zeile darin (mit Liste); sonst sagt ein Toast,
   * wo die Task liegt, wenn das nicht die angezeigte Liste ist (mit Sprung dorthin).
   */
  function announce(title: string, targetId: string) {
    const short = title.length > 40 ? `${title.slice(0, 39)}…` : title;
    if (keepOpen) {
      created = `„${short}“ in „${listLabel(targetId)}“ angelegt`;
      return;
    }
    if (shownListId === undefined || targetId === shownListId) return;
    toasts.show({
      message: `„${short}“ in „${listLabel(targetId)}“ angelegt`,
      actionLabel: onopenlist ? "Öffnen" : undefined,
      onAction: onopenlist ? () => onopenlist(targetId) : undefined,
    });
  }

  /** Nach dem Anlegen bei „Weitere anlegen“: alles außer der Listenwahl zurücksetzen. */
  function reset() {
    value = "";
    notes = "";
    dueOpen = false;
    manualDue = undefined;
    manualPriority = undefined;
    scroll = 0;
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

  function syncScroll() {
    scroll = input?.scrollLeft ?? 0;
  }
</script>

<Dialog title="Neue Task" {onclose} width={560} bare placement="top">
  <div class="body" data-new-task>
    <div class="composer">
      <div class="title-field">
        <!-- Markierungsebene: derselbe Text, erkannte Stellen getönt; das Feld liegt transparent darüber. -->
        <div class="mirror" aria-hidden="true">
          <span class="mirror-text" style:transform="translateX({-scroll}px)">
            {#each segments as seg, i (i)}
              {#if seg.mark}<mark class="mark {seg.mark.kind} {seg.mark.priority ?? ''}">{seg.text}</mark>{:else}{seg.text}{/if}
            {/each}
          </span>
        </div>
        <input
          bind:this={input}
          bind:value
          placeholder="Was ist zu tun? z. B. „Bericht fr 14:00 !1 @Arbeit“"
          aria-label="Neue Task"
          maxlength="500"
          autocomplete="off"
          spellcheck="true"
          oninput={() => {
            created = null;
            syncScroll();
          }}
          onscroll={syncScroll}
          onkeyup={syncScroll}
          onclick={syncScroll}
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
      <textarea
        bind:this={notesField}
        bind:value={notes}
        class="notes"
        rows="1"
        aria-label="Notiz"
        placeholder="Notiz hinzufügen …"
        maxlength="20000"
        oninput={() => (created = null)}
        onkeydown={(e) => {
          if (e.key === "Enter" && e.ctrlKey) {
            e.preventDefault();
            void submit();
          } else if (e.key === "Escape") {
            e.preventDefault();
            onclose();
          }
        }}
      ></textarea>
    </div>

    <div class="chips">
      <span class="chip-group" class:set={!!due}>
        <button
          bind:this={dueButton}
          type="button"
          class="chip {dueTone ?? ''}"
          aria-haspopup="dialog"
          aria-expanded={dueOpen}
          data-tooltip="Fälligkeit   Alt+D"
          onclick={toggleDue}
        >
          <CalendarDays size={14} strokeWidth={1.75} aria-hidden="true" />{due ? dueChipText(due) : "Fällig"}
        </button>
        {#if due}
          <button type="button" class="chip-x" aria-label="Fälligkeit entfernen" onclick={() => pickDue(null)}>
            <X size={12} strokeWidth={2} aria-hidden="true" />
          </button>
        {/if}
      </span>
      <span class="chip-group" class:set={priority !== "none"}>
        <button
          bind:this={prioButton}
          type="button"
          class="chip"
          aria-haspopup="menu"
          aria-expanded={prioMenu !== null}
          aria-label="Priorität: {prioLabel}"
          data-tooltip="Priorität   Alt+P"
          onclick={openPriority}
        >
          <span class="ring {priority}" aria-hidden="true"></span>{priority === "none" ? "Priorität" : prioLabel}
        </button>
        {#if priority !== "none"}
          <button
            type="button"
            class="chip-x"
            aria-label="Priorität entfernen"
            onclick={() => {
              manualPriority = "none";
              input?.focus();
            }}
          >
            <X size={12} strokeWidth={2} aria-hidden="true" />
          </button>
        {/if}
      </span>
      <ListPicker bind:this={picker} {lists} value={target} onchange={chooseList} shortcut="Alt+L" />
    </div>

    {#if created}<p class="created" role="status"><Check size={14} strokeWidth={2} aria-hidden="true" />{created}</p>{/if}

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
      <span class="actions">
        <IconButton label="Eingabehilfe" icon={CircleHelp} size={28} data-tooltip={SYNTAX} tooltipSide="top" />
        {#if !density.narrow}<kbd class="esc">Esc</kbd>{/if}
        <Button
          variant={parsed.title ? "primary" : "secondary"}
          size={density.narrow ? "lg" : undefined}
          disabled={!parsed.title}
          onclick={() => void submit()}
        >
          Anlegen<kbd class="enter" aria-hidden="true">↵</kbd>
        </Button>
      </span>
    </footer>
  </div>

  {#if dueOpen && dueButton}
    <DuePopover
      anchor={dueButton}
      value={due}
      onpick={pickDue}
      onclose={(refocus) => {
        dueOpen = false;
        if (refocus) input?.focus();
      }}
    />
  {/if}
  <!-- Im Dialog rendern: außerhalb läge das Menü hinter der modalen Ebene. -->
  {#if prioMenu}
    <Menu label="Priorität wählen" items={prioItems()} x={prioMenu.x} y={prioMenu.y} onclose={() => (prioMenu = null)} />
  {/if}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .composer {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    transition:
      border-color var(--duration-fast) var(--ease),
      box-shadow var(--duration-fast) var(--ease);
  }
  .composer:hover {
    border-color: var(--color-border-strong);
  }
  .composer:focus-within {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
  .title-field {
    position: relative;
    height: 44px;
  }
  /* Feld und Markierungsebene teilen Schrift, Größe und Innenabstand – sonst verrutschen die Markierungen. */
  .title-field input,
  .mirror {
    padding: 0 var(--space-3);
    font: inherit;
    font-size: var(--text-lg);
    letter-spacing: normal;
  }
  .title-field input {
    position: relative;
    width: 100%;
    height: 100%;
    border: 0;
    background: transparent;
    color: var(--color-text);
  }
  .title-field input:focus {
    outline: none;
  }
  .title-field input::placeholder {
    color: var(--color-text-muted);
  }
  .mirror {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    overflow: hidden;
    color: transparent;
    white-space: pre;
    pointer-events: none;
  }
  .mirror-text {
    display: inline-block;
  }
  .mark {
    --tint: var(--color-accent);
    border-radius: 3px;
    background: color-mix(in srgb, var(--tint) 18%, transparent);
    /* Fläche etwas über die Buchstaben hinaus, ohne den Text zu verschieben. */
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--tint) 18%, transparent);
    color: transparent;
  }
  .mark.priority.high {
    --tint: var(--color-danger);
  }
  .mark.priority.medium {
    --tint: var(--color-warning);
  }
  .mark.list {
    --tint: var(--color-text-muted);
  }
  .notes {
    display: block;
    width: 100%;
    min-height: 32px;
    max-height: 40vh;
    padding: 0 var(--space-3) var(--space-3);
    border: 0;
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    line-height: 1.5;
    resize: none;
    field-sizing: content;
  }
  .notes:focus {
    outline: none;
  }
  .notes::placeholder {
    color: var(--color-text-muted);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  /* Ein Chip: leer nur Kontur, gesetzt getönt mit × (P6-03). */
  .chip-group {
    display: inline-flex;
    align-items: center;
    height: 28px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: var(--color-bg);
    transition: background-color var(--duration-fast) var(--ease);
  }
  .chip-group.set {
    border-color: color-mix(in srgb, var(--color-accent) 30%, transparent);
    background: var(--color-bg-selected);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 100%;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    cursor: pointer;
  }
  .set .chip {
    padding-right: var(--space-1);
    color: var(--color-text);
  }
  .chip:hover,
  .chip[aria-expanded="true"] {
    color: var(--color-text);
  }
  .chip-group:not(.set):hover {
    background: var(--color-bg-hover);
  }
  .chip:focus-visible,
  .chip-x:focus-visible {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: 1px;
  }
  .set .chip.overdue {
    color: var(--color-danger);
  }
  .set .chip.today {
    color: var(--color-accent-text);
  }
  .chip-x {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-right: 3px;
    border: 0;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }
  .chip-x:hover {
    background: var(--color-bg-hover);
    color: var(--color-text);
  }
  /* Priorität als Ring wie in Liste und Detail. */
  .ring {
    --ring: var(--color-control-border);
    width: 10px;
    height: 10px;
    border: 1.5px solid var(--ring);
    border-radius: var(--radius-full);
  }
  .ring.high,
  .ring.medium,
  .ring.low {
    background: color-mix(in srgb, var(--ring) 12%, transparent);
  }
  .ring.high {
    --ring: var(--color-danger);
  }
  .ring.medium {
    --ring: var(--color-warning);
  }
  .ring.low {
    --ring: var(--color-accent);
  }
  .created {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--color-success);
    font-size: var(--text-sm);
    animation: created-in var(--duration-base) var(--ease);
  }
  @keyframes created-in {
    from {
      opacity: 0;
    }
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
  .actions {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  kbd {
    font: inherit;
    font-size: var(--text-xs);
  }
  .esc {
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--color-bg-subtle);
    color: var(--color-text-muted);
  }
  .enter {
    margin-left: 6px;
    opacity: 0.75;
  }
  /* Schmal: Chips in einer waagerecht scrollbaren Reihe. */
  @media (max-width: 699px) {
    .chips {
      flex-wrap: nowrap;
      overflow-x: auto;
      margin: 0 calc(-1 * var(--space-4));
      padding: 2px var(--space-4);
      scrollbar-width: none;
    }
    .chips > :global(*) {
      flex: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .created {
      animation: none;
    }
  }
</style>
