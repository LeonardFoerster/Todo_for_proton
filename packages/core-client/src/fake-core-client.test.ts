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

  it("emits change events with exactly the written ids", async () => {
    const { core } = await setup();
    const events: ChangedEventDto[] = [];
    const off = core.onChange((e) => events.push(e));
    const a = await core.createTask({ title: "a" });
    await core.completeTask(a.id, true);
    await core.completeTask(a.id, true); // keine Änderung → kein Event
    await Promise.resolve();
    expect(events).toEqual([
      { tasks: [a.id], lists: [] },
      { tasks: [a.id], lists: [] },
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
});
