import { FakeCoreClient, type TaskSummaryDto } from "@tasks/core-client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toasts } from "../components/toasts.svelte";
import { BULK_LIMIT, completeManyWithUndo, deleteManyWithUndo, moveManyWithUndo } from "./task-actions";

beforeEach(() => {
  toasts.items = [];
});

const N = BULK_LIMIT + 500;

async function setup() {
  const client = new FakeCoreClient();
  const inbox = await client.ensureInbox();
  const work = await client.createTaskList("Arbeit");
  for (let i = 0; i < N; i++) await client.createTask({ title: `T${String(i).padStart(4, "0")}` });
  const all = async (listId: string) => {
    const items: TaskSummaryDto[] = [];
    let cursor: string | null = null;
    do {
      const page = await client.listTasks({ view: { kind: "list", listId } }, { cursor, limit: 500 });
      items.push(...page.items);
      cursor = page.nextCursor;
    } while (cursor);
    return items;
  };
  const undo = async () => {
    toasts.items.at(-1)!.onAction!();
    await vi.waitFor(() => expect(toasts.items.some((t) => t.tone === "danger")).toBe(false));
  };
  return { client, inbox, work, all, undo };
}

describe("bulk actions above the core limit", () => {
  it("deletes and restores more than 2 000 tasks in chunks, keeping the order", async () => {
    const { client, inbox, all, undo } = await setup();
    const before = (await all(inbox.id)).map((t) => t.title);
    const del = vi.spyOn(client, "deleteTasks");
    const restore = vi.spyOn(client, "restoreTasks");
    const deleted = await deleteManyWithUndo(
      client,
      (await all(inbox.id)).map((t) => t.id),
    );
    expect(deleted).toHaveLength(N);
    expect(del).toHaveBeenCalledTimes(2);
    expect(await all(inbox.id)).toHaveLength(0);
    await undo();
    await vi.waitFor(async () => expect((await all(inbox.id)).map((t) => t.title)).toEqual(before));
    expect(restore).toHaveBeenCalledTimes(2);
  });

  it("completes in chunks", async () => {
    const { client, inbox, all } = await setup();
    const done = await completeManyWithUndo(
      client,
      (await all(inbox.id)).map((t) => t.id),
      true,
    );
    expect(done).toHaveLength(N);
    expect((await all(inbox.id)).every((t) => t.status === "done")).toBe(true);
  });

  it("moves more than 2 000 tasks as one ordered block and back", async () => {
    const { client, inbox, work, all, undo } = await setup();
    const tasks = await all(inbox.id);
    const titles = tasks.map((t) => t.title);
    const move = vi.spyOn(client, "moveTasks");
    const moved = await moveManyWithUndo(client, tasks, { listId: work.id }, "verschoben");
    expect(moved).toHaveLength(N);
    expect(move).toHaveBeenCalledTimes(2);
    expect((await all(work.id)).map((t) => t.title)).toEqual(titles);
    await undo();
    await vi.waitFor(async () => expect((await all(inbox.id)).map((t) => t.title)).toEqual(titles));
    expect(await all(work.id)).toHaveLength(0);
  });
});
