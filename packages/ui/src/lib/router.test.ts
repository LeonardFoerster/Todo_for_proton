import { describe, expect, it } from "vitest";
import { localDay } from "./local-day";
import { parseHash, routeHash } from "./router.svelte";

describe("router", () => {
  it("parses and formats routes", () => {
    expect(parseHash("")).toEqual({ view: "inbox" });
    expect(parseHash("#/today")).toEqual({ view: "today" });
    expect(parseHash("#/list/abc-1")).toEqual({ view: "list", listId: "abc-1" });
    expect(parseHash("#/bogus")).toEqual({ view: "inbox" });
    expect(parseHash("#/list/%E0%A4%A")).toEqual({ view: "inbox" });
    expect(routeHash({ view: "list", listId: "a b" })).toBe("#/list/a%20b");
    expect(parseHash(routeHash({ view: "list", listId: "a b" }))).toEqual({ view: "list", listId: "a b" });
  });
});

describe("localDay", () => {
  it("uses local midnight boundaries", () => {
    const day = localDay(new Date(2026, 9, 18, 23, 59));
    expect(day.date).toBe("2026-10-18");
    expect(new Date(day.start).getDate()).toBe(18);
    expect(new Date(day.end).getTime() - new Date(day.start).getTime()).toBeGreaterThanOrEqual(23 * 3600e3);
  });
});
