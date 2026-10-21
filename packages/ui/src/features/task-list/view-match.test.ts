import { describe, expect, it } from "vitest";
import { compareInView, shownInView, type TaskLike, type ViewContext } from "./view-match";

const day = { date: "2026-10-18", start: "2026-10-17T22:00:00.000Z", end: "2026-10-18T22:00:00.000Z" };
const ctx = (route: ViewContext["route"]): ViewContext => ({ route, inboxId: "inbox", day });
const task = (over: Partial<TaskLike> = {}): TaskLike => ({
  id: "t",
  listId: "inbox",
  status: "open",
  due: null,
  sortKey: "a0",
  completedAt: null,
  trashedAt: null,
  ...over,
});

describe("view matching", () => {
  it("matches lists and the inbox", () => {
    expect(shownInView(task(), ctx({ view: "inbox" }))).toBe(true);
    expect(shownInView(task({ listId: "w" }), ctx({ view: "inbox" }))).toBe(false);
    expect(shownInView(task({ listId: "w" }), ctx({ view: "list", listId: "w" }))).toBe(true);
    expect(shownInView(task({ status: "done" }), ctx({ view: "inbox" }))).toBe(false);
  });

  it("splits today and upcoming at the end of the local day", () => {
    const today = task({ due: { kind: "date", date: "2026-10-18" } });
    const overdue = task({ due: { kind: "date", date: "2026-10-01" } });
    const later = task({ due: { kind: "dateTime", at: "2026-10-18T22:30:00.000Z", tz: "Europe/Berlin" } });
    expect(shownInView(today, ctx({ view: "today" }))).toBe(true);
    expect(shownInView(overdue, ctx({ view: "today" }))).toBe(true);
    expect(shownInView(later, ctx({ view: "today" }))).toBe(false);
    expect(shownInView(later, ctx({ view: "upcoming" }))).toBe(true);
    expect(shownInView(task(), ctx({ view: "upcoming" }))).toBe(false);
  });

  it("orders completed by completion time, others by sort key", () => {
    const a = task({ id: "a", sortKey: "a1", completedAt: "2026-10-18T10:00:00Z", status: "done" });
    const b = task({ id: "b", sortKey: "a0", completedAt: "2026-10-18T11:00:00Z", status: "done" });
    expect(compareInView(a, b, { view: "inbox" })).toBeGreaterThan(0);
    expect(compareInView(a, b, { view: "completed" })).toBeGreaterThan(0);
    expect(compareInView(b, a, { view: "completed" })).toBeLessThan(0);
  });
});
