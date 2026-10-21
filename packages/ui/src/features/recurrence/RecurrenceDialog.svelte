<script lang="ts">
  /**
   * „Benutzerdefiniert…“ für die Wiederholung (N4-05): Intervall und Einheit, Wochentage (wöchentlich),
   * Bezug (fester Rhythmus oder nach Erledigung) und Ende. Ergebnis über `onpick`; `Esc` bricht ab.
   */
  import type { DueDto, RecurrenceDto, WeekdayDto } from "@tasks/core-client";
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import TextInput from "../../components/TextInput.svelte";
  import { parseIsoDate } from "../../lib/local-day";
  import { describeRule, rule, WEEKDAY_SHORT, WEEKDAYS, weekdayOf } from "../../lib/recurrence";
  import { parseDue } from "../new-task/parse";

  interface Props {
    value: RecurrenceDto | null;
    /** Fälligkeit, an der sich die Regel verankert (Vorgabe für den Wochentag). */
    due: DueDto | null;
    onpick: (rule: RecurrenceDto) => void;
    onclose: () => void;
  }

  let { value, due, onpick, onclose }: Props = $props();

  const UNITS: { freq: RecurrenceDto["freq"]; one: string; many: string }[] = [
    { freq: "daily", one: "Tag", many: "Tage" },
    { freq: "weekly", one: "Woche", many: "Wochen" },
    { freq: "monthly", one: "Monat", many: "Monate" },
    { freq: "yearly", one: "Jahr", many: "Jahre" },
  ];
  // Startwerte beim Öffnen – spätere Änderungen von außen gibt es nicht (modal).
  // svelte-ignore state_referenced_locally
  const dueDay = due ? (due.kind === "date" ? parseIsoDate(due.date) : new Date(due.at)) : new Date();
  // svelte-ignore state_referenced_locally
  const start = value ?? rule("weekly");

  let freq = $state(start.freq);
  let interval = $state(String(start.interval));
  let weekdays = $state<WeekdayDto[]>(start.weekdays.length > 0 ? [...start.weekdays] : [weekdayOf(dueDay)]);
  let anchor = $state(start.anchor);
  let ends = $state(!!start.until);
  let untilText = $state(start.until ? formatDay(start.until) : "");

  const count = $derived(Number(interval));
  const intervalOk = $derived(Number.isInteger(count) && count >= 1 && count <= 999);
  const untilDue = $derived(ends ? parseDue(untilText) : null);
  const until = $derived(untilDue?.kind === "date" ? untilDue.date : null);
  const result = $derived<RecurrenceDto | null>(
    intervalOk && (!ends || until)
      ? {
          freq,
          interval: count,
          weekdays: freq === "weekly" ? WEEKDAYS.filter((d) => weekdays.includes(d)) : [],
          monthDay: freq === "monthly" || freq === "yearly" ? (start.freq === freq ? start.monthDay : null) : null,
          anchor,
          until: ends ? until : null,
        }
      : null,
  );

  function formatDay(iso: string): string {
    const d = parseIsoDate(iso);
    return `${d.getDate()}.${d.getMonth() + 1}.${d.getFullYear()}`;
  }

  function toggleDay(day: WeekdayDto) {
    // Mindestens ein Tag bleibt gewählt.
    if (weekdays.includes(day)) {
      if (weekdays.length > 1) weekdays = weekdays.filter((d) => d !== day);
    } else weekdays = [...weekdays, day];
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (result) onpick(result);
  }
</script>

<Dialog title="Wiederholung" {onclose} width={400}>
  <form class="form" onsubmit={submit}>
    <div class="row">
      <label for="rec-interval">Alle</label>
      <TextInput id="rec-interval" bind:value={interval} inputmode="numeric" maxlength={3} invalid={!intervalOk} />
      <select aria-label="Einheit" bind:value={freq}>
        {#each UNITS as u (u.freq)}<option value={u.freq}>{count === 1 ? u.one : u.many}</option>{/each}
      </select>
    </div>

    {#if freq === "weekly"}
      <div class="days" role="group" aria-label="Wochentage">
        {#each WEEKDAYS as day (day)}
          <button type="button" class="day" aria-pressed={weekdays.includes(day)} onclick={() => toggleDay(day)}>
            {WEEKDAY_SHORT[day]}
          </button>
        {/each}
      </div>
    {/if}

    <fieldset>
      <legend>Bezug</legend>
      <label><input type="radio" name="anchor" value="schedule" bind:group={anchor} /> Fester Rhythmus</label>
      <label><input type="radio" name="anchor" value="completion" bind:group={anchor} /> Nach Erledigung</label>
    </fieldset>

    <div class="row">
      <label class="ends"><input type="checkbox" bind:checked={ends} /> Endet am</label>
      <TextInput
        aria-label="Ende"
        placeholder="z. B. 20.12."
        bind:value={untilText}
        disabled={!ends}
        invalid={ends && !until}
      />
    </div>

    <p class="summary" aria-live="polite">{result ? describeRule(result) : "Bitte Angaben prüfen"}</p>

    <footer>
      <Button onclick={onclose}>Abbrechen</Button>
      <Button type="submit" variant="primary" disabled={!result}>Übernehmen</Button>
    </footer>
  </form>
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .row :global(.input) {
    width: 5.5em;
  }
  .row .ends {
    white-space: nowrap;
  }
  .row .ends + :global(.input) {
    width: auto;
    flex: 1;
  }
  select {
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
  }
  .days {
    display: flex;
    gap: var(--space-1);
  }
  .day {
    flex: 1;
    height: 32px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .day[aria-pressed="true"] {
    border-color: var(--color-accent);
    background: var(--color-bg-selected);
    color: var(--color-accent);
    font-weight: 600;
  }
  fieldset {
    display: flex;
    gap: var(--space-4);
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    margin-bottom: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  fieldset label,
  .ends {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  .summary {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
