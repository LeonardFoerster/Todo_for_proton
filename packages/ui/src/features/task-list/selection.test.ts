import { FakeCoreClient, type TaskSummaryDto } from "@tasks/core-client";
import { describe, expect, it } from "vitest";
import { SelectionState } from "./selection.svelte";
import { TaskListState } from "./task-list-state.svelte";

const order = ["a", "b", "c", "d", "e", "f"];
const selected = (s: SelectionState) => order.filter((id) => s.has(id)).join("");

describe("SelectionState", () => {
  it("enters, toggles and exits", () => {
    const s = new SelectionState();
    s.enter(["c"]);
    expect(s.active).toBe(true);
    expect(selected(s)).toBe("c");
    s.toggle("a");
    s.toggle("c");
    expect(selected(s)).toBe("a");
    expect(s.count).toBe(1);
    expect(s.anchor).toBe("c");
    s.exit();
    expect(s.active).toBe(false);
    expect(s.count).toBe(0);
    expect(s.anchor).toBeNull();
  });

  it("adds ranges forwards and backwards from the anchor", () => {
    const s = new SelectionState();
    s.enter();
    s.toggle("b");
    s.range("d", order);
    expect(selected(s)).toBe("bcd");
    s.toggle("f");
    s.range("e", order);
    expect(selected(s), "rückwärts ab f").toBe("bcdef");
    s.toggle("c");
    s.range("a", order);
    expect(selected(s), "Bereich fügt nur hinzu").toBe("abcdef");
  });

  it("uses the cursor row when there is no anchor", () => {
    const s = new SelectionState();
    s.enter();
    s.range("e", order, "c");
    expect(selected(s)).toBe("cde");
    expect(s.anchor).toBe("c");
    const t = new SelectionState();
    t.range("b", order);
    expect(selected(t), "ohne Anker und Cursor nur das Ziel").toBe("b");
    t.anchor = "gone";
    t.range("d", order, null);
    expect(selected(t)).toBe("bd");
  });

  it("extends with Shift+arrows and stops at the edges", () => {
    const s = new SelectionState();
    expect(s.extend(1, "b", order)).toBe("c");
    expect(selected(s)).toBe("bc");
    expect(s.extend(-1, "a", order)).toBe("a");
    expect(selected(s)).toBe("abc");
    expect(s.extend(1, "f", order)).toBe("f");
    expect(selected(s)).toBe("abcf");
    const t = new SelectionState();
    expect(t.extend(1, null, order), "ohne Cursor: erste Zeile").toBe("a");
    expect(t.extend(-1, null, [])).toBeNull();
  });

  it("prunes ids that left the view", () => {
    const s = new SelectionState();
    s.enter(["a", "b", "c"]);
    s.prune(new Set(["a", "c", "x"]));
    expect(selected(s)).toBe("ac");
    expect(s.anchor).toBe("c");
    s.prune(new Set(["a"]));
    expect(s.anchor).toBeNull();
  });

  it("reports open and done tasks among the selection", () => {
    const rows = new Map<string, TaskSummaryDto>(
      ["a", "b"].map((id, i) => [id, { id, status: i === 0 ? "open" : "done" } as TaskSummaryDto]),
    );
    const s = new SelectionState();
    s.enter(["a", "unknown"]);
    expect(s.flags((id) => rows.get(id))).toEqual({ hasOpen: true, hasDone: false });
    s.toggle("b");
    expect(s.flags((id) => rows.get(id))).toEqual({ hasOpen: true, hasDone: true });
    s.clear();
    expect(s.flags((id) => rows.get(id))).toEqual({ hasOpen: false, hasDone: false });
  });
});

describe("TaskListState.loadAll", () => {
  it("loads the remaining pages so that select all covers the whole view", async () => {
    const client = new FakeCoreClient();
    const inbox = await client.ensureInbox();
    for (let i = 0; i < 1234; i++) await client.createTask({ title: `T${i}` });
    const day = { date: "2026-10-18", start: "2026-10-17T22:00:00.000Z", end: "2026-10-18T22:00:00.000Z" };
    const rows = new TaskListState(client, () => ({ route: { view: "inbox" }, inboxId: inbox.id, day }));
    await rows.load();
    expect(rows.items).toHaveLength(200);
    expect(rows.hasMore).toBe(true);

    const ids = await rows.loadAll();
    expect(ids).toHaveLength(1234);
    expect(new Set(ids).size).toBe(1234);
    expect(rows.hasMore).toBe(false);
    expect(rows.byId(ids[1233])?.title).toBe("T1233");

    const s = new SelectionState();
    s.enter();
    s.selectAll(ids);
    expect(s.count).toBe(1234);
    expect(await rows.loadAll()).toHaveLength(1234);

    // Bereich und Umschalt+↓ über den O(1)-Index der geladenen Zeilen.
    s.clear();
    expect(rows.indexOf(ids[1000])).toBe(1000);
    s.range(ids[1003], rows.order(), ids[1000]);
    expect(s.count).toBe(4);
    expect(s.extend(1, ids[1233], rows.order())).toBe(ids[1233]);
    expect(rows.indexOf("unknown")).toBe(-1);
  });
});

describe("TaskListState.reorderMany", () => {
  async function state(titles = "ABCDEF") {
    const client = new FakeCoreClient();
    const inbox = await client.ensureInbox();
    for (const t of titles) await client.createTask({ title: t });
    const day = { date: "2026-10-18", start: "2026-10-17T22:00:00.000Z", end: "2026-10-18T22:00:00.000Z" };
    const rows = new TaskListState(client, () => ({ route: { view: "inbox" }, inboxId: inbox.id, day }));
    await rows.load();
    const id = (t: string) => rows.items.find((r) => r.title === t)!.id;
    const title = (i?: string) => rows.items.find((r) => r.id === i)?.title;
    const move = (block: string, to: number) => {
      const n = rows.reorderMany(new Set([...block].map(id)), to);
      return n && `${title(n.after) ?? "-"}|${title(n.before) ?? "-"}`;
    };
    const order = () => rows.items.map((r) => r.title).join("");
    return { move, order };
  }

  it("moves a contiguous block up and down", async () => {
    const s = await state();
    expect(s.move("CD", 1)).toBe("A|B");
    expect(s.order()).toBe("ACDBEF");
    expect(s.move("CD", 6)).toBe("F|-");
    expect(s.order()).toBe("ABEFCD");
  });

  it("gathers a scattered selection at the insertion point", async () => {
    const s = await state();
    expect(s.move("BE", 3)).toBe("C|D");
    expect(s.order()).toBe("ACBEDF");
    expect(s.move("AF", 0)).toBe("-|C");
    expect(s.order()).toBe("AFCBED");
  });

  it("moves to the start and the end", async () => {
    const s = await state();
    expect(s.move("DE", 0)).toBe("-|A");
    expect(s.order()).toBe("DEABCF");
    expect(s.move("DA", 6)).toBe("F|-");
    expect(s.order()).toBe("EBCFDA");
  });

  it("returns null when the block lands on itself", async () => {
    const s = await state();
    for (const to of [2, 3, 4]) expect(s.move("CD", to)).toBeNull();
    expect(s.order()).toBe("ABCDEF");
    expect(s.move("BD", 2), "verstreut über sich selbst: wird zusammengezogen").toBe("A|C");
    expect(s.order()).toBe("ABDCEF");
    expect(s.move("", 0)).toBeNull();
  });
});
