import { describe, expect, it } from "vitest";
import { CoreClientError } from "./errors";
import { FakeCoreClient } from "./fake-core-client";
import type { ChangedEventDto, LocalDayDto, TaskQueryDto } from "./types";

const inbox: TaskQueryDto = { view: { kind: "inbox" } };
const day: LocalDayDto = { date: "2026-10-18", start: "2026-10-17T22:00:00.000Z", end: "2026-10-18T22:00:00.000Z" };

async function setup() {
  const core = new FakeCoreClient({ now: () => new Date("2026-10-18T10:00:00.000Z") });
  const inboxList = await core.ensureInbox();
  return { core, inboxList };
}

async function titles(core: FakeCoreClient, query: TaskQueryDto = inbox) {
  const page = await core.listTasks(query, { limit: 500 });
  return page.items.map((t) => t.title);
}

describe("FakeCoreClient", () => {
  it("creates the inbox exactly once", async () => {
    const { core, inboxList } = await setup();
    expect(inboxList.isInbox).toBe(true);
    await core.ensureInbox();
    expect(await core.listTaskLists()).toHaveLength(1);
  });

  it("creates tasks in the inbox in insertion order and validates titles", async () => {
    const { core } = await setup();
    const a = await core.createTask({ title: "  Milch  " });
    await core.createTask({ title: "Brot" });
    expect(a.title).toBe("Milch");
    expect(a.status).toBe("open");
    expect(await titles(core)).toEqual(["Milch", "Brot"]);
    await expect(core.createTask({ title: "   " })).rejects.toMatchObject({ kind: "validation" });
    await expect(core.createTask({ title: "x".repeat(501) })).rejects.toBeInstanceOf(CoreClientError);
  });

  it("creates tasks with initial notes, priority and due", async () => {
    const { core } = await setup();
    const t = await core.createTask({
      title: "Blatt 3",
      notes: "Aufgabe 1–4",
      priority: "high",
      due: { kind: "date", date: "2026-10-23" },
    });
    expect(t).toMatchObject({ notes: "Aufgabe 1–4", priority: "high", due: { kind: "date", date: "2026-10-23" } });
  });

  it("emits change events with exactly the written ids", async () => {
    const { core } = await setup();
    const events: ChangedEventDto[] = [];
    const off = core.onChange((e) => events.push(e));
    const a = await core.createTask({ title: "a" });
    await core.completeTask(a.id, true);
    await core.completeTask(a.id, true); // keine Änderung → kein Event
    await Promise.resolve();
    expect(events).toEqual([
      { tasks: [a.id], lists: [], notes: [], noteFolders: [] },
      { tasks: [a.id], lists: [], notes: [], noteFolders: [] },
    ]);
    off();
    await core.deleteTask(a.id);
    await Promise.resolve();
    expect(events).toHaveLength(2);
  });

  it("updates fields; due null removes, missing due keeps", async () => {
    const { core } = await setup();
    const a = await core.createTask({ title: "a" });
    let t = await core.updateTask(a.id, { notes: "n", priority: "high", due: { kind: "date", date: "2026-10-20" } });
    expect(t).toMatchObject({ notes: "n", priority: "high", due: { kind: "date", date: "2026-10-20" } });
    t = await core.updateTask(a.id, { title: "b" });
    expect(t.due).not.toBeNull();
    t = await core.updateTask(a.id, { due: null });
    expect(t.due).toBeNull();
    const [row] = (await core.listTasks(inbox, { limit: 10 })).items;
    expect(row.hasNotes).toBe(true);
  });

  it("completes and reopens with completedAt", async () => {
    const { core } = await setup();
    const a = await core.createTask({ title: "a" });
    expect((await core.completeTask(a.id, true)).completedAt).toBe("2026-10-18T10:00:00.000Z");
    expect((await core.completeTask(a.id, false)).completedAt).toBeNull();
  });

  it("moves before/after/between and to the end of another list", async () => {
    const { core } = await setup();
    const [a, b, c] = [await core.createTask({ title: "a" }), await core.createTask({ title: "b" }), await core.createTask({ title: "c" })];
    await core.moveTask(c.id, { before: a.id });
    expect(await titles(core)).toEqual(["c", "a", "b"]);
    await core.moveTask(c.id, { after: b.id });
    expect(await titles(core)).toEqual(["a", "b", "c"]);
    await core.moveTask(a.id, { after: b.id, before: c.id });
    expect(await titles(core)).toEqual(["b", "a", "c"]);
    await expect(core.moveTask(a.id, { after: a.id })).rejects.toMatchObject({ kind: "validation" });

    const work = await core.createTaskList("Arbeit");
    await core.moveTask(b.id, { listId: work.id });
    expect(await titles(core)).toEqual(["a", "c"]);
    expect(await titles(core, { view: { kind: "list", listId: work.id } })).toEqual(["b"]);
  });

  it("filters smart views and status", async () => {
    const { core } = await setup();
    const today = await core.createTask({ title: "heute" });
    const later = await core.createTask({ title: "später" });
    const done = await core.createTask({ title: "erledigt" });
    await core.updateTask(today.id, { due: { kind: "date", date: "2026-10-18" } });
    await core.updateTask(later.id, { due: { kind: "date", date: "2026-10-25" } });
    await core.completeTask(done.id, true);
    expect(await titles(core, { view: { kind: "today", day } })).toEqual(["heute"]);
    expect(await titles(core, { view: { kind: "upcoming", day } })).toEqual(["später"]);
    expect(await titles(core, { view: { kind: "completed" } })).toEqual(["erledigt"]);
    expect(await titles(core, { view: { kind: "all" }, status: "open" })).toEqual(["heute", "später"]);
  });

  it("pages with an opaque cursor", async () => {
    const { core } = await setup();
    for (let i = 0; i < 5; i++) await core.createTask({ title: `t${i}` });
    const first = await core.listTasks(inbox, { limit: 2 });
    expect(first.items.map((t) => t.title)).toEqual(["t0", "t1"]);
    const second = await core.listTasks(inbox, { cursor: first.nextCursor, limit: 10 });
    expect(second.items.map((t) => t.title)).toEqual(["t2", "t3", "t4"]);
    expect(second.nextCursor).toBeNull();
    await expect(core.listTasks(inbox, { limit: 0 })).rejects.toMatchObject({ kind: "validation" });
  });

  it("deleting a list moves its tasks to the inbox; the inbox is protected", async () => {
    const { core, inboxList } = await setup();
    const work = await core.createTaskList("Arbeit");
    await core.createTask({ title: "x", listId: work.id });
    expect(await core.deleteTaskList(work.id)).toBe(1);
    expect(await titles(core)).toEqual(["x"]);
    await expect(core.deleteTaskList(inboxList.id)).rejects.toMatchObject({ kind: "validation" });
    await expect(core.renameTaskList(inboxList.id, "y")).rejects.toMatchObject({ kind: "validation" });
    await expect(core.getTask("nope")).rejects.toMatchObject({ kind: "notFound" });
  });

  it("logs in after pending polls and reports sync status", async () => {
    const core = new FakeCoreClient({ loginPolls: 1 });
    const states: string[] = [];
    core.onSyncStatus((s) => states.push(s.state));
    expect(await core.auth.current()).toBeNull();
    const step = await core.auth.beginLogin();
    if (step.kind !== "openBrowser") throw new Error("expected openBrowser");
    expect((await core.auth.pollLogin(step.flow)).kind).toBe("pending");
    const done = await core.auth.pollLogin(step.flow);
    expect(done.kind).toBe("done");
    expect(await core.auth.current()).toEqual({ accountLabel: "demo@example.invalid" });
    await core.auth.logout();
    expect(states).toEqual(["idle", "disabled"]);
    await expect(core.auth.pollLogin(step.flow)).rejects.toMatchObject({ kind: "auth" });
  });

  describe("bulk operations", () => {
    async function bulkSetup() {
      const { core } = await setup();
      const work = await core.createTaskList("Arbeit");
      const ids: Record<string, string> = {};
      for (const t of "abcdef") ids[t] = (await core.createTask({ title: t })).id;
      for (const t of "xyz") ids[t] = (await core.createTask({ title: t, listId: work.id })).id;
      const events: ChangedEventDto[] = [];
      core.onChange((e) => events.push(e));
      await Promise.resolve();
      const flush = async () => {
        await Promise.resolve();
        return events.splice(0);
      };
      const list = { view: { kind: "list", listId: work.id } } as TaskQueryDto;
      return { core, work, ids, flush, list, of: (s: string) => [...s].map((c) => ids[c]) };
    }

    it("moves a block in display order to start, middle and end with one event each", async () => {
      const { core, flush, of } = await bulkSetup();
      await core.moveTasks(of("eb"), { before: of("a")[0] });
      expect((await titles(core)).join("")).toBe("beacdf");
      expect(await flush()).toEqual([{ tasks: of("be"), lists: [], notes: [], noteFolders: [] }]);
      await core.moveTasks(of("fb"), { after: of("c")[0], before: of("d")[0] });
      expect((await titles(core)).join("")).toBe("eacbfd");
      await core.moveTasks(of("ea"), {});
      expect((await titles(core)).join("")).toBe("cbfdea");
      expect(await flush()).toHaveLength(2);
    });

    it("moves into another list and rejects invalid targets", async () => {
      const { core, work, of, list } = await bulkSetup();
      const moved = await core.moveTasks(of("db"), { listId: work.id, after: of("x")[0] });
      expect(moved.map((t) => t.title)).toEqual(["b", "d"]);
      expect((await titles(core, list)).join("")).toBe("xbdyz");
      await expect(core.moveTasks(of("ax"), {})).rejects.toMatchObject({ kind: "validation" });
      await expect(core.moveTasks(of("ac"), { after: of("c")[0] })).rejects.toMatchObject({ kind: "validation" });
      await expect(core.moveTasks(of("a"), { listId: "nope" })).rejects.toMatchObject({ kind: "notFound" });
    });

    it("completes and reopens, skipping unknown ids and tasks already in the target state", async () => {
      const { core, flush, of } = await bulkSetup();
      await core.completeTask(of("a")[0], true);
      await flush();
      const done = await core.completeTasks([...of("abc"), "unknown", ...of("b")], true);
      expect(done.map((t) => t.title)).toEqual(["b", "c"]);
      expect(await flush()).toEqual([{ tasks: of("bc"), lists: [], notes: [], noteFolders: [] }]);
      expect(await core.completeTasks(of("bc"), true)).toEqual([]);
      expect(await flush()).toEqual([]);
      expect((await core.completeTasks(of("abc"), false)).every((t) => t.completedAt === null)).toBe(true);
      await expect(core.completeTasks(Array(2001).fill("x"), true)).rejects.toMatchObject({ kind: "validation" });
    });

    it("deletes with snapshots and restores them with new ids at the old position", async () => {
      const { core, flush, of } = await bulkSetup();
      await core.completeTask(of("c")[0], true);
      await flush();
      const deleted = await core.deleteTasks([...of("ec"), "unknown"]);
      expect(deleted.map((t) => t.title)).toEqual(["e", "c"]);
      expect((await titles(core)).join("")).toBe("abdf");
      expect(await flush()).toEqual([{ tasks: of("ec"), lists: [], notes: [], noteFolders: [] }]);

      const restored = await core.restoreTasks(deleted);
      expect(restored.map((t) => t.id)).not.toContain(of("c")[0]);
      expect((await titles(core)).join("")).toBe("abcdef");
      expect(restored[1]).toMatchObject({ title: "c", status: "done", createdAt: deleted[1].createdAt });
      expect(await flush()).toEqual([{ tasks: restored.map((t) => t.id), lists: [], notes: [], noteFolders: [] }]);
    });

    it("restores into the inbox when the list is gone", async () => {
      const { core, work, of } = await bulkSetup();
      const deleted = await core.deleteTasks(of("y"));
      await core.deleteTaskList(work.id);
      const [restored] = await core.restoreTasks(deleted);
      expect(restored.listId).toBe((await core.ensureInbox()).id);
    });
  });

  it("reorders lists with the inbox always first and one event", async () => {
    const { core } = await setup();
    const a = await core.createTaskList("A");
    const b = await core.createTaskList("B");
    const c = await core.createTaskList("C");
    const names = async () => (await core.listTaskLists()).map((l) => l.name).join(",");
    const events: ChangedEventDto[] = [];
    core.onChange((e) => events.push(e));
    await core.moveTaskList(c.id, { before: a.id });
    expect(await names()).toBe("Inbox,C,A,B");
    await core.moveTaskList(c.id, { after: b.id });
    expect(await names()).toBe("Inbox,A,B,C");
    await core.moveTaskList(a.id, {});
    expect(await names()).toBe("Inbox,B,C,A");
    await Promise.resolve();
    expect(events).toEqual([
      { tasks: [], lists: [c.id], notes: [], noteFolders: [] },
      { tasks: [], lists: [c.id], notes: [], noteFolders: [] },
      { tasks: [], lists: [a.id], notes: [], noteFolders: [] },
    ]);
    const inbox = (await core.listTaskLists())[0];
    await expect(core.moveTaskList(inbox.id, {})).rejects.toMatchObject({ kind: "validation" });
    await expect(core.moveTaskList(a.id, { before: inbox.id })).rejects.toMatchObject({ kind: "validation" });
    const d = await core.createTaskList("D");
    expect(await names(), "neue Liste ans Ende").toBe("Inbox,B,C,A,D");
    expect(d.sortKey > (await core.listTaskLists())[3].sortKey).toBe(true);
  });

  it("sorts Today/Upcoming by due (all-day first) and sets dues in one call", async () => {
    const { core } = await setup();
    const a = await core.createTask({ title: "a" });
    const b = await core.createTask({ title: "b" });
    const c = await core.createTask({ title: "c" });
    const events: ChangedEventDto[] = [];
    core.onChange((e) => events.push(e));
    const changed = await core.setDues([
      { id: a.id, due: { kind: "dateTime", at: "2026-10-18T08:00:00.000Z", tz: "Europe/Berlin" } },
      { id: b.id, due: { kind: "date", date: "2026-10-18" } },
      { id: c.id, due: { kind: "date", date: "2026-10-16" } },
      { id: "unbekannt", due: null },
    ]);
    expect(changed.map((t) => t.title)).toEqual(["a", "b", "c"]);
    expect(events).toHaveLength(1);
    expect(await titles(core, { view: { kind: "today", day } })).toEqual(["c", "b", "a"]);
    expect(await core.setDues([{ id: b.id, due: { kind: "date", date: "2026-10-18" } }])).toEqual([]);
    await expect(core.setDues([{ id: a.id, due: null }, { id: b.id, due: { kind: "date", date: "x" } }])).rejects.toMatchObject({
      kind: "validation",
    });
    expect((await core.getTask(a.id)).due?.kind, "nichts geschrieben").toBe("dateTime");
  });

  it("searches title and notes like the core (all words, open and title hits first)", async () => {
    const { core } = await setup();
    await core.createTask({ title: "Notiz", notes: "Projekt Alpha" });
    const done = await core.createTask({ title: "Projekt alt" });
    await core.createTask({ title: "Übungsblatt Projekt" });
    await core.completeTask(done.id, true);
    expect((await core.searchTasks("projekt", 10)).map((t) => t.title)).toEqual(["Übungsblatt Projekt", "Notiz", "Projekt alt"]);
    expect((await core.searchTasks("PROJEKT alpha", 10)).map((t) => t.title)).toEqual(["Notiz"]);
    expect((await core.searchTasks("übung", 10)).map((t) => t.title)).toEqual(["Übungsblatt Projekt"]);
    expect(await core.searchTasks("  ", 10)).toEqual([]);
    await expect(core.searchTasks("x", 0)).rejects.toMatchObject({ kind: "validation" });
  });
});
