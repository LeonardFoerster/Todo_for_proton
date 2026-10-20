import { describe, expect, it } from "vitest";
import { viewSubtitle } from "./subtitle";

const now = new Date(2026, 9, 21, 9, 0);

describe("viewSubtitle", () => {
  it("shows date, open and overdue in Heute", () => {
    expect(viewSubtitle({ view: "today" }, { inbox: 0, today: 3, overdue: 1 }, now)).toBe(
      "Mittwoch, 21. Oktober · 3 offen · 1 überfällig",
    );
    expect(viewSubtitle({ view: "today" }, { inbox: 0, today: 0, overdue: 0 }, now)).toBe(
      "Mittwoch, 21. Oktober · Nichts offen",
    );
  });

  it("counts the inbox and stays empty where no count is known", () => {
    expect(viewSubtitle({ view: "inbox" }, { inbox: 4, today: 0, overdue: 0 }, now)).toBe("4 offen");
    expect(viewSubtitle({ view: "upcoming" }, { inbox: 4, today: 0, overdue: 0 }, now)).toBe("");
  });
});
