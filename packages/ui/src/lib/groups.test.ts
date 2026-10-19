import type { DueDto } from "@tasks/core-client";
import { describe, expect, it } from "vitest";
import { groupEntries, isOverdue, weekdayDate, type Entry } from "./groups";

const now = new Date(2026, 9, 19, 12, 0); // Montag, 19. Oktober 2026, 12:00
const date = (d: string): DueDto => ({ kind: "date", date: d });
const at = (y: number, m: number, d: number, h: number): DueDto => ({
  kind: "dateTime",
  at: new Date(y, m - 1, d, h).toISOString(),
  tz: "Europe/Berlin",
});
const t = (title: string, due: DueDto | null) => ({ title, due });

/** Kompakte Darstellung: Überschriften als „# Label (n)“, Tasks als „Titel/Modus“. */
function show(entries: Entry<{ title: string; due: DueDto | null }>[] | null): string[] | null {
  return entries?.map((e) => (e.kind === "header" ? `# ${e.label} (${e.count})` : `${e.task.title}/${e.dueMode}`)) ?? null;
}

describe("isOverdue", () => {
  it("treats dates from tomorrow on and past times as overdue like dueLabel", () => {
    expect(isOverdue(date("2026-10-18"), now)).toBe(true);
    expect(isOverdue(date("2026-10-19"), now)).toBe(false);
    expect(isOverdue(at(2026, 10, 19, 9), now)).toBe(true);
    expect(isOverdue(at(2026, 10, 19, 15), now)).toBe(false);
  });
});

describe("groupEntries – Heute", () => {
  it("splits overdue (first, full date) from today (time only)", () => {
    // Eingabe nach Fälligkeit sortiert (wie aus Core + compareInView).
    const items = [t("alt", date("2026-10-10")), t("gestern", date("2026-10-18")), t("ganztägig", date("2026-10-19")), t("9 Uhr", at(2026, 10, 19, 9)), t("15 Uhr", at(2026, 10, 19, 15))];
    expect(show(groupEntries(items, { view: "today" }, now))).toEqual([
      "# Überfällig (3)",
      "alt/full",
      "gestern/full",
      "9 Uhr/full",
      "# Heute (2)",
      "ganztägig/time",
      "15 Uhr/time",
    ]);
  });

  it("has no headings without overdue tasks and no empty Heute section", () => {
    expect(show(groupEntries([t("a", date("2026-10-19"))], { view: "today" }, now))).toEqual(["a/time"]);
    expect(show(groupEntries([t("a", date("2026-10-01"))], { view: "today" }, now))).toEqual(["# Überfällig (1)", "a/full"]);
  });

  it("does not group other views", () => {
    expect(groupEntries([t("a", null)], { view: "inbox" }, now)).toBeNull();
    expect(groupEntries([t("a", null)], { view: "all" }, now)).toBeNull();
  });
});

describe("groupEntries – Geplant", () => {
  it("groups tomorrow, the next days, rest of the month and months", () => {
    const items = [
      t("morgen", date("2026-10-20")),
      t("morgen 8", at(2026, 10, 20, 8)),
      t("mi", date("2026-10-21")),
      t("so", date("2026-10-25")),
      t("später", date("2026-10-29")),
      t("nov", date("2026-11-03")),
      t("nov2", date("2026-11-30")),
      t("jan", date("2027-01-04")),
    ];
    expect(show(groupEntries(items, { view: "upcoming" }, now))).toEqual([
      "# Morgen (2)",
      "morgen/time",
      "morgen 8/time",
      `# ${weekdayDate(new Date(2026, 9, 21))} (1)`,
      "mi/time",
      `# ${weekdayDate(new Date(2026, 9, 25))} (1)`,
      "so/time",
      "# Später in diesem Monat (1)",
      "später/full",
      "# November (2)",
      "nov/full",
      "nov2/full",
      "# Januar 2027 (1)",
      "jan/full",
    ]);
    expect(weekdayDate(new Date(2026, 9, 21))).toBe("Mi, 21. Okt.");
  });

  it("starts the next month right after the single days near the month end", () => {
    const end = new Date(2026, 9, 28, 9); // Mi, 28. Okt.
    const items = [t("so", date("2026-11-01")), t("di", date("2026-11-03")), t("mi", date("2026-11-04"))];
    expect(show(groupEntries(items, { view: "upcoming" }, end))).toEqual([
      `# ${weekdayDate(new Date(2026, 10, 1))} (1)`,
      "so/time",
      `# ${weekdayDate(new Date(2026, 10, 3))} (1)`,
      "di/time",
      "# November (1)",
      "mi/full",
    ]);
  });

  it("keeps the local day across the DST change (25.10.)", () => {
    const sat = new Date(2026, 9, 24, 23, 30);
    const items = [t("so 0:30", at(2026, 10, 25, 0)), t("mo", at(2026, 10, 26, 0))];
    expect(show(groupEntries(items, { view: "upcoming" }, sat))).toEqual([
      "# Morgen (1)",
      "so 0:30/time",
      `# ${weekdayDate(new Date(2026, 9, 26))} (1)`,
      "mo/time",
    ]);
  });
});
