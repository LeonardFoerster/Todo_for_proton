import { describe, expect, it } from "vitest";
import { FakeCoreClient } from "./fake-core-client";
import { autoName } from "./fake-notes";
import type { ChangedEventDto, NoteTreeItemDto } from "./types";

function setup() {
  return new FakeCoreClient({ now: () => new Date("2026-10-20T10:00:00.000Z") });
}

async function names(core: FakeCoreClient, folderId: string | null, limit = 500): Promise<string[]> {
  const out: string[] = [];
  let cursor: string | null = null;
  do {
    const page = await core.notes.listItems(folderId, { cursor, limit });
    out.push(...page.items.map((i: NoteTreeItemDto) => `${i.type === "folder" ? "F" : "N"}:${i.name}`));
    cursor = page.nextCursor;
  } while (cursor);
  return out;
}

describe("FakeNotes", () => {
  it("creates untitled notes, follows the first line and stops after a rename", async () => {
    const core = setup();
    const note = await core.notes.create(null, "markdown");
    expect(note).toMatchObject({ name: "Unbenannt", autoName: true, body: "", bodyRev: null });
    const saved = await core.notes.updateBody(note.id, "# Blatt 3\r\nText", note.bodyRev);
    expect(saved.body).toBe("# Blatt 3\nText");
    expect(saved.name).toBe("Blatt 3");
    expect(saved.bodyRev).not.toBeNull();
    const renamed = await core.notes.rename(note.id, "Analysis");
    expect(renamed.autoName).toBe(false);
    expect((await core.notes.updateBody(note.id, "# Anders", renamed.bodyRev)).name).toBe("Analysis");
    await expect(core.notes.rename(note.id, "a/b")).rejects.toMatchObject({ kind: "validation" });
    await expect(core.notes.get("nope")).rejects.toMatchObject({ kind: "notFound" });
  });

  it("auto names like the core", () => {
    expect(autoName("")).toBe("Unbenannt");
    expect(autoName("\n- [ ] Milch kaufen")).toBe("Milch kaufen");
    expect(autoName("...geheim")).toBe("geheim");
    expect(autoName("a/b")).toBe("a-b");
  });

  it("mixes folders and notes in one order, pages and moves", async () => {
    const core = setup();
    const a = await core.notes.create(null, "text");
    const uni = await core.notes.createFolder(null, "Uni");
    const b = await core.notes.create(null, "text");
    await core.notes.rename(a.id, "a");
    await core.notes.rename(b.id, "b");
    expect(await names(core, null, 1)).toEqual(["N:a", "F:Uni", "N:b"]);
    await core.notes.move(b.id, { folderId: null, before: { type: "note", id: a.id } });
    expect(await names(core, null)).toEqual(["N:b", "N:a", "F:Uni"]);
    await core.notes.move(a.id, { folderId: uni.id });
    expect(await names(core, uni.id)).toEqual(["N:a"]);
    const sub = await core.notes.createFolder(uni.id, "Analysis");
    await expect(core.notes.moveFolder(uni.id, { folderId: sub.id })).rejects.toMatchObject({ kind: "validation" });
    await core.notes.moveFolder(sub.id, { folderId: null, after: { type: "note", id: b.id } });
    expect(await names(core, null)).toEqual(["N:b", "F:Analysis", "F:Uni"]);
  });

  it("trash is read-only, lists top items and restores into the root when the folder is gone", async () => {
    const core = setup();
    const uni = await core.notes.createFolder(null, "Uni");
    const note = await core.notes.create(uni.id, "markdown");
    await core.notes.updateBody(note.id, "Text", null);
    expect(await core.notes.trash([{ type: "note", id: note.id }])).toHaveLength(1);
    await core.notes.trash([{ type: "folder", id: uni.id }]);
    const trash = await core.notes.listTrash();
    expect(trash.map((e) => e.item.name)).toEqual(["Uni"]);
    await expect(core.notes.updateBody(note.id, "x", null)).rejects.toMatchObject({ kind: "validation" });
    await core.notes.restore([{ type: "note", id: note.id }]);
    expect((await core.notes.get(note.id)).folderId).toBeNull();
    expect(await core.notes.emptyTrash()).toBe(1);
    expect(await core.notes.listFolders()).toEqual([]);
  });

  it("purges folders with their content and discards empty new notes", async () => {
    const core = setup();
    const uni = await core.notes.createFolder(null, "Uni");
    const sub = await core.notes.createFolder(uni.id, "Sub");
    await core.notes.create(sub.id, "text");
    expect(await core.notes.purge([{ type: "folder", id: uni.id }])).toBe(3);
    const empty = await core.notes.create(null, "text");
    expect(await core.notes.discardIfEmpty(empty.id)).toBe(true);
    expect(await core.notes.discardIfEmpty(empty.id)).toBe(false);
  });

  it("searches names and bodies, names first, short words only in names", async () => {
    const core = setup();
    const folder = await core.notes.createFolder(null, "Uni");
    const a = await core.notes.create(folder.id, "markdown");
    await core.notes.updateBody(a.id, "# Analysis\nGrenzwerte von Folgen", null);
    const b = await core.notes.create(null, "markdown");
    await core.notes.updateBody(b.id, "# Folgen\nKonvergenz", null);
    expect((await core.notes.search("folgen", 10)).map((h) => h.note.name)).toEqual(["Folgen", "Analysis"]);
    const [hit] = await core.notes.search("grenz", 10);
    expect(hit.path).toEqual(["Uni"]);
    expect(hit.snippet).toMatchObject({ offset: 11, length: 5 });
    expect(await core.notes.search("ve", 10)).toEqual([]);
    expect((await core.notes.search("fo", 10)).map((h) => h.note.name)).toEqual(["Folgen"]);
  });

  it("reports writes as note events and simulates remote bodies", async () => {
    const core = setup();
    const events: ChangedEventDto[] = [];
    core.onChange((e) => events.push(e));
    const note = await core.notes.create(null, "text");
    const folder = await core.notes.createFolder(null, "F");
    core.notes.simulateRemoteBody(note.id, "von B");
    await Promise.resolve();
    expect(events).toEqual([
      { tasks: [], lists: [], notes: [note.id], noteFolders: [], tags: [], links: [] },
      { tasks: [], lists: [], notes: [], noteFolders: [folder.id], tags: [], links: [] },
      { tasks: [], lists: [], notes: [note.id], noteFolders: [], tags: [], links: [] },
    ]);
    expect((await core.notes.get(note.id)).body).toBe("von B");
    const conflicts: string[] = [];
    core.notes.onConflict((c) => conflicts.push(c[0].copyId));
    core.notes.simulateConflict([{ noteId: note.id, copyId: "kopie" }]);
    await Promise.resolve();
    expect(conflicts).toEqual(["kopie"]);
  });
});
