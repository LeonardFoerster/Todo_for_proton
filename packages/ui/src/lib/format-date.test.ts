import { describe, expect, it } from "vitest";
import { dueLabel, todayPlus } from "./format-date";

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
