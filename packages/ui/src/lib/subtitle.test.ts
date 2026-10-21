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

  it("counts lists, tags, all and upcoming via countTasks (FIX-04)", () => {
    const counts = { inbox: 0, today: 0, overdue: 0, all: 7, upcoming: 2, lists: { l: 3 }, tags: { t: 0 } };
    expect(viewSubtitle({ view: "list", listId: "l" }, counts, now)).toBe("3 offen");
    expect(viewSubtitle({ view: "tag", tagId: "t" }, counts, now)).toBe("Nichts offen");
    expect(viewSubtitle({ view: "all" }, counts, now)).toBe("7 offen");
    expect(viewSubtitle({ view: "upcoming" }, counts, now)).toBe("2 geplant");
    expect(viewSubtitle({ view: "list", listId: "unbekannt" }, counts, now)).toBe("");
  });
});
