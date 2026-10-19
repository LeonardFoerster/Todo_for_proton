import { describe, expect, it } from "vitest";
import { dueFromInputs, dueLabel, dueToInputDate, dueToInputTime, keepTime, todayPlus } from "./format-date";

const now = new Date(2026, 9, 18, 10, 0); // Sonntag, 18. Oktober 2026
const date = (d: string) => ({ kind: "date" as const, date: d });

describe("dueLabel", () => {
  it("uses relative words near today", () => {
    expect(dueLabel(date("2026-10-18"), now)).toMatchObject({ text: "Heute", tone: "today" });
    expect(dueLabel(date("2026-10-19"), now)).toMatchObject({ text: "Morgen", tone: "soon" });
    expect(dueLabel(date("2026-10-17"), now)).toMatchObject({ text: "Gestern", tone: "overdue" });
  });

  it("uses weekdays within a week and dates beyond", () => {
    expect(dueLabel(date("2026-10-21"), now).text).toBe("Mi");
    expect(dueLabel(date("2026-11-03"), now)).toMatchObject({ text: "3. Nov.", tone: "later" });
    expect(dueLabel(date("2027-01-05"), now).text).toBe("5. Jan. 2027");
    expect(dueLabel(date("2026-10-10"), now).tone).toBe("overdue");
  });

  it("shows times and treats past times today as overdue", () => {
    const at = new Date(2026, 9, 18, 9, 30).toISOString();
    const label = dueLabel({ kind: "dateTime", at, tz: "Europe/Berlin" }, now);
    expect(label.text).toBe("Heute 09:30");
    expect(label.tone).toBe("overdue");
    expect(label.long).toContain("überfällig");
  });

  it("builds date dues relative to today", () => {
    expect(todayPlus(1, now)).toEqual(date("2026-10-19"));
  });
});

describe("Datum + Uhrzeit ↔ DueDto", () => {
  const at = (y: number, m: number, d: number, h: number, min: number) =>
    ({ kind: "dateTime" as const, at: new Date(y, m - 1, d, h, min).toISOString(), tz: "Europe/Berlin" });

  it("round-trips date and time through the input fields", () => {
    const due = dueFromInputs("2026-10-20", "14:05", "Europe/Berlin")!;
    expect(due).toEqual(at(2026, 10, 20, 14, 5));
    expect(dueToInputDate(due)).toBe("2026-10-20");
    expect(dueToInputTime(due)).toBe("14:05");
  });

  it("is all-day without time and nothing without date", () => {
    expect(dueFromInputs("2026-10-20", "")).toEqual(date("2026-10-20"));
    expect(dueFromInputs("", "14:00")).toBeNull();
    expect(dueToInputTime(date("2026-10-20"))).toBe("");
    expect(dueToInputTime(null)).toBe("");
  });

  it("keeps the time when only the day changes", () => {
    const current = at(2026, 10, 20, 9, 30);
    expect(keepTime(date("2026-10-23"), current)).toEqual(at(2026, 10, 23, 9, 30));
    expect(keepTime(date("2026-10-23"), date("2026-10-20"))).toEqual(date("2026-10-23"));
    expect(keepTime(date("2026-10-23"), null)).toEqual(date("2026-10-23"));
    expect(keepTime(null, current)).toBeNull();
  });

  it("keeps the local time across the DST change", () => {
    // 24.10. (Sommerzeit) → 26.10. (Winterzeit): weiterhin 09:30 lokal.
    const moved = keepTime(date("2026-10-26"), at(2026, 10, 24, 9, 30))!;
    expect(dueToInputTime(moved)).toBe("09:30");
    expect(dueToInputDate(moved)).toBe("2026-10-26");
  });
});
