import type { NoteTreeItemDto } from "@tasks/core-client";
import { describe, expect, it } from "vitest";
import { ago, dropTarget, flatten, parentIndex, ROOT, siblingTarget } from "./tree-model";

const folder = (id: string, parentId: string | null = null): NoteTreeItemDto => ({
  type: "folder",
  id,
  parentId,
  name: id,
  sortKey: id,
  createdAt: "",
  trashedAt: null,
});
const note = (id: string, folderId: string | null = null): NoteTreeItemDto => ({
  type: "note",
  id,
  folderId,
  name: id,
  kind: "markdown",
  sortKey: id,
  createdAt: "",
  modifiedAt: "",
  trashedAt: null,
  size: 0,
});

const children = new Map<string, NoteTreeItemDto[]>([
  [ROOT, [folder("uni"), note("einkauf")]],
  ["uni", [folder("ana", "uni"), note("plan", "uni")]],
  ["ana", [note("blatt", "ana")]],
]);

describe("tree model", () => {
  it("flattens expanded folders with depth", () => {
    const rows = flatten(children, new Set(["uni", "ana"]));
    expect(rows.map((r) => `${r.depth}:${r.item.id}`)).toEqual(["0:uni", "1:ana", "2:blatt", "1:plan", "0:einkauf"]);
    expect(flatten(children, new Set(["ana"])).map((r) => r.item.id)).toEqual(["uni", "einkauf"]);
    expect(parentIndex(rows, 2)).toBe(1);
    expect(parentIndex(rows, 0)).toBe(-1);
  });

  it("finds sibling targets for Alt+↑/↓", () => {
    const rows = flatten(children, new Set(["uni"]));
    // rows: uni, ana, plan, einkauf
    expect(siblingTarget(rows, 2, -1)).toEqual({ folderId: "uni", before: { type: "folder", id: "ana" } });
    expect(siblingTarget(rows, 1, -1)).toBeNull();
    expect(siblingTarget(rows, 0, 1)).toEqual({ folderId: null, after: { type: "note", id: "einkauf" } });
  });

  it("computes drop targets: into folders, between rows, never into the own subtree", () => {
    const rows = flatten(children, new Set(["uni"]));
    const h = 32;
    expect(dropTarget(rows, 16, h, "n:einkauf")).toEqual({ kind: "into", folderId: "uni", rowIndex: 0 });
    expect(dropTarget(rows, 3 * h + 2, h, "f:ana")).toEqual({
      kind: "between",
      folderId: null,
      before: { type: "note", id: "einkauf" },
      line: 3 * h,
    });
    expect(dropTarget(rows, 2 * h + 28, h, "n:einkauf")).toEqual({
      kind: "between",
      folderId: "uni",
      after: { type: "note", id: "plan" },
      line: 3 * h,
    });
    // Ordner in sich selbst bzw. seinen Inhalt: kein Ziel.
    expect(dropTarget(rows, 16, h, "f:uni")).toBeNull();
    expect(dropTarget(rows, 2 * h + 4, h, "f:uni")).toBeNull();
    // Unter einem aufgeklappten Ordner: vor sein erstes Kind.
    expect(dropTarget(rows, 30, h, "n:einkauf")).toEqual({
      kind: "between",
      folderId: "uni",
      before: { type: "folder", id: "ana" },
      line: h,
    });
  });

  it("formats trash ages", () => {
    const now = new Date("2026-10-20T12:00:00Z");
    expect(ago("2026-10-20T11:59:50Z", now)).toBe("gerade eben");
    expect(ago("2026-10-20T11:00:00Z", now)).toBe("vor 1 Stunde");
    expect(ago("2026-10-17T12:00:00Z", now)).toBe("vor 3 Tagen");
  });
});
