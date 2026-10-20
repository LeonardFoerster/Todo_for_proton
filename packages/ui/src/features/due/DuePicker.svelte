<script lang="ts">
  /**
   * Fälligkeit wählen (P5-08): Text wie in der Titelzeile von „Neue Task“ („fr“, „24.10.“, „morgen 14:00“,
   * derselbe Parser `parseDue`), Schnellwahl, Monatsraster und Uhrzeit. Ersetzt die nativen Datums-/Zeitfelder
   * im Detail, im Fenster „Neue Task“ und im Dialog „Datum wählen…“.
   *
   * `onchange` bekommt die fertige Fälligkeit: Ein reiner Tag (Raster, Schnellwahl, „fr“) behält die Uhrzeit
   * von `value`; die Uhrzeit setzt bzw. leert genau die Uhrzeit.
   */
  import type { DueDto } from "@tasks/core-client";
  import { tick, untrack } from "svelte";
  import IconButton from "../../components/IconButton.svelte";
  import { ChevronLeft, ChevronRight, X } from "../../icons";
  import { dueFromInputs, dueLabel, dueToInputDate, dueToInputTime, keepTime } from "../../lib/format-date";
  import { addDays, isoDate, parseIsoDate } from "../../lib/local-day";
  import { parseDue } from "../new-task/parse";
  import { dueChoices } from "./due-actions";

  interface Props {
    value: DueDto | null;
    onchange: (due: DueDto | null) => void;
    /** „Entfernen“ anbieten (nicht im Dialog „Datum wählen…“, dort steht es im Menü). */
    removable?: boolean;
    /** Uhrzeit anbieten. */
    withTime?: boolean;
    /** Texteingabe beim Öffnen fokussieren. */
    autofocus?: boolean;
  }

  let { value, onchange, removable = true, withTime = true, autofocus = false }: Props = $props();

  const uid = $props.id();
  const hintId = `due-picker-hint-${uid}`;
  const WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
  const MONTH = new Intl.DateTimeFormat("de-DE", { month: "long", year: "numeric" });
  const DAY_LONG = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long", year: "numeric" });

  /** Momentaufnahme beim Öffnen. */
  const now = new Date();
  const today = isoDate(now);
  const selected = $derived(dueToInputDate(value));
  /** Startmonat: der gewählte Tag, sonst heute (nur beim Öffnen). */
  const start = untrack(() => (selected ? parseIsoDate(selected) : now));

  let text = $state("");
  let error = $state<string | null>(null);
  let time = $state(untrack(() => dueToInputTime(value)));
  /** Zuletzt übernommene Uhrzeit: `change` beim Verlassen nach `Enter` meldet sie nicht noch einmal. */
  let committedTime = untrack(() => time);
  let timeError = $state<string | null>(null);
  /** Angezeigter Monat und der Tag mit Tastaturfokus im Raster (roving tabindex). */
  let month = $state(new Date(start.getFullYear(), start.getMonth(), 1));
  let focusDay = $state(isoDate(start));
  let grid: HTMLDivElement;
  let input: HTMLInputElement;

  const parsed = $derived(text.trim() ? parseDue(text) : null);
  const preview = $derived(parsed ? dueLabel(parsed).long : null);
  const days = $derived.by(() => {
    const first = addDays(month, -((month.getDay() + 6) % 7));
    return Array.from({ length: 42 }, (_, i) => addDays(first, i));
  });
  const choices = dueChoices(now).filter((c) => c.id !== "none");

  // Erst nach dem Einhängen der Umgebung: `Dialog` fokussiert beim Öffnen sich selbst.
  $effect(() => {
    if (autofocus) void tick().then(() => input?.focus());
  });

  function pickDay(day: DueDto) {
    onchange(keepTime(day, value));
  }

  function submitText() {
    if (!parsed) {
      error = "Kein Datum erkannt. Beispiele: fr, 24.10., morgen 14:00";
      return;
    }
    onchange(keepTime(parsed, value));
  }

  function submitTime() {
    const t = time.trim();
    if (t === committedTime) return;
    if (!t) {
      committedTime = t;
      if (value) onchange({ kind: "date", date: selected });
      return;
    }
    const m = /^(\d{1,2})(?:[:.](\d{2}))?$/.exec(t);
    const [h, min] = m ? [Number(m[1]), Number(m[2] ?? 0)] : [NaN, NaN];
    if (!(h < 24 && min < 60)) {
      timeError = "Uhrzeit wie 14:00 eingeben";
      return;
    }
    const hhmm = `${String(h).padStart(2, "0")}:${String(min).padStart(2, "0")}`;
    time = committedTime = hhmm;
    onchange(dueFromInputs(selected || today, hhmm, value?.kind === "dateTime" ? value.tz : undefined));
  }

  function showMonth(delta: number) {
    month = new Date(month.getFullYear(), month.getMonth() + delta, 1);
  }

  /** Pfeile/Bild/Pos1/Ende im Raster (ARIA-Grid-Muster); wechselt bei Bedarf den Monat. */
  async function onGridKey(e: KeyboardEvent) {
    const day = parseIsoDate(focusDay);
    let next: Date | null = null;
    if (e.key === "ArrowLeft") next = addDays(day, -1);
    else if (e.key === "ArrowRight") next = addDays(day, 1);
    else if (e.key === "ArrowUp") next = addDays(day, -7);
    else if (e.key === "ArrowDown") next = addDays(day, 7);
    else if (e.key === "Home") next = addDays(day, -((day.getDay() + 6) % 7));
    else if (e.key === "End") next = addDays(day, 6 - ((day.getDay() + 6) % 7));
    else if (e.key === "PageUp" || e.key === "PageDown") {
      const delta = e.key === "PageUp" ? -1 : 1;
      const last = new Date(day.getFullYear(), day.getMonth() + delta + 1, 0).getDate();
      next = new Date(day.getFullYear(), day.getMonth() + delta, Math.min(day.getDate(), last));
    }
    if (!next) return;
    e.preventDefault();
    e.stopPropagation();
    focusDay = isoDate(next);
    if (next.getMonth() !== month.getMonth() || next.getFullYear() !== month.getFullYear()) {
      month = new Date(next.getFullYear(), next.getMonth(), 1);
    }
    await tick();
    grid.querySelector<HTMLButtonElement>(`[data-date="${focusDay}"]`)?.focus();
  }
</script>

<div class="picker">
  <input
    bind:this={input}
    bind:value={text}
    class="field text"
    aria-label="Fällig am"
    aria-describedby={hintId}
    aria-invalid={error ? true : undefined}
    placeholder="fr, 24.10., morgen 14:00"
    oninput={() => (error = null)}
    onkeydown={(e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        submitText();
      }
    }}
  />
  <p class="hint" id={hintId} aria-live="polite">{preview ?? ""}</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <div class="choices">
    {#each choices as choice (choice.id)}
      <button type="button" class="chip" onclick={() => pickDay(choice.due!)}>{choice.label}</button>
    {/each}
  </div>

  <div class="month">
    <span class="month-name" aria-live="polite">{MONTH.format(month)}</span>
    <IconButton label="Voriger Monat" shortcut="Bild↑" icon={ChevronLeft} size={28} onclick={() => showMonth(-1)} />
    <IconButton label="Nächster Monat" shortcut="Bild↓" icon={ChevronRight} size={28} onclick={() => showMonth(1)} />
  </div>
  <div class="grid" role="grid" aria-label={MONTH.format(month)} bind:this={grid} tabindex="-1" onkeydown={onGridKey}>
    <div class="row" role="row">
      {#each WEEKDAYS as wd (wd)}<span class="wd" role="columnheader">{wd}</span>{/each}
    </div>
    {#each [0, 1, 2, 3, 4, 5] as week (week)}
      <div class="row" role="row">
        {#each days.slice(week * 7, week * 7 + 7) as day (day.getTime())}
          {@const iso = isoDate(day)}
          <button
            type="button"
            role="gridcell"
            class="day"
            class:outside={day.getMonth() !== month.getMonth()}
            class:today={iso === today}
            aria-selected={iso === selected}
            aria-current={iso === today ? "date" : undefined}
            aria-label={DAY_LONG.format(day)}
            tabindex={iso === focusDay ? 0 : -1}
            data-date={iso}
            onfocus={() => (focusDay = iso)}
            onclick={() => pickDay({ kind: "date", date: iso })}
          >
            {day.getDate()}
          </button>
        {/each}
      </div>
    {/each}
  </div>

  {#if withTime || (removable && value)}
    <div class="foot">
      {#if withTime}
        <input
          bind:value={time}
          class="field time"
          aria-label="Uhrzeit"
          aria-invalid={timeError ? true : undefined}
          placeholder="Uhrzeit"
          inputmode="numeric"
          oninput={() => (timeError = null)}
          onchange={submitTime}
          onkeydown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              e.stopPropagation();
              submitTime();
            }
          }}
        />
      {/if}
      {#if removable && value}
        <button type="button" class="chip ghost" onclick={() => onchange(null)}>
          <X size={14} strokeWidth={1.75} aria-hidden="true" />Entfernen
        </button>
      {/if}
    </div>
    {#if timeError}<p class="error" role="alert">{timeError}</p>{/if}
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 100%;
  }
  .field {
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    font-variant-numeric: tabular-nums;
  }
  .field:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-focus-ring);
  }
  .field::placeholder {
    color: var(--color-text-muted);
  }
  .text {
    width: 100%;
  }
  .time {
    width: 96px;
  }
  .hint,
  .error {
    margin: 0;
    font-size: var(--text-xs);
  }
  .hint {
    color: var(--color-text-muted);
  }
  .hint:empty {
    display: none;
  }
  .error {
    color: var(--color-danger);
  }
  .choices,
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }
  .foot {
    justify-content: space-between;
    padding-top: var(--space-2);
    border-top: 1px solid var(--color-border);
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
  }
  .chip:hover {
    background: var(--color-bg-hover);
  }
  .chip.ghost {
    border-color: transparent;
    background: transparent;
    color: var(--color-text-muted);
  }
  .chip.ghost:hover {
    background: var(--color-bg-hover);
    color: var(--color-danger);
  }
  .month {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-top: var(--space-1);
  }
  .month-name {
    flex: 1;
    padding-left: var(--space-1);
    font-weight: var(--weight-semibold);
  }
  .grid {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .grid:focus {
    outline: none;
  }
  .row {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 2px;
  }
  .wd {
    padding: 2px 0;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    text-align: center;
  }
  .day {
    position: relative;
    height: 32px;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .day:hover {
    background: var(--color-bg-hover);
  }
  .day.outside {
    color: var(--color-text-muted);
  }
  .day.today {
    color: var(--color-accent-text);
    font-weight: var(--weight-semibold);
  }
  .day.today::after {
    content: "";
    position: absolute;
    left: 50%;
    bottom: 4px;
    width: 4px;
    height: 4px;
    margin-left: -2px;
    border-radius: var(--radius-full);
    background: currentColor;
  }
  .day[aria-selected="true"] {
    background: var(--color-accent-fill);
    color: var(--color-on-accent);
  }
  .day:focus-visible {
    outline: 2px solid var(--color-focus-outline);
    outline-offset: 1px;
  }
</style>
