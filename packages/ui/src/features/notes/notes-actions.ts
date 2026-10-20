/**
 * Aktionen im Notiz-Baum mit Rückmeldung (Toasts, Rückgängig). Die UI ruft nur `CoreClient`.
 */
import type { NoteItemDto, NoteKindDto, NoteTargetDto, NoteTreeItemDto } from "@tasks/core-client";
import { toasts } from "../../components/toasts.svelte";
import type { NotesState } from "./notes-state.svelte";
import { itemRef } from "./tree-model";

function fail(what: string, e: unknown) {
  toasts.show({ message: `${what}: ${(e as Error).message}`, tone: "danger" });
}

function label(item: NoteTreeItemDto): string {
  return item.type === "folder" ? `Ordner „${item.name}“` : `„${item.name}“`;
}

/** In den Papierkorb (Toast mit „Rückgängig"); eine offene Notiz wird geschlossen. */
export async function trashWithUndo(notes: NotesState, items: NoteTreeItemDto[]) {
  const refs = items.map(itemRef);
  try {
    const changed = await notes.client.notes.trash(refs);
    if (changed.length === 0) return;
    if (items.some((i) => i.type === "note" && i.id === notes.openId)) notes.open(null);
    await notes.reload();
    toasts.show({
      message: items.length === 1 ? `${label(items[0])} in den Papierkorb gelegt` : `${items.length} Elemente in den Papierkorb gelegt`,
      actionLabel: "Rückgängig",
      onAction: () => void restore(notes, changed),
    });
  } catch (e) {
    fail("Nicht in den Papierkorb gelegt", e);
  }
}

export async function restore(notes: NotesState, items: NoteItemDto[]) {
  try {
    await notes.client.notes.restore(items);
    await notes.reload();
  } catch (e) {
    fail("Nicht wiederhergestellt", e);
  }
}

/** Umbenennen; ein gleichnamiges Element im selben Ordner ist erlaubt, aber einen Hinweis wert. */
export async function rename(notes: NotesState, item: NoteTreeItemDto, name: string, parentId: string | null) {
  const trimmed = name.trim();
  if (!trimmed || trimmed === item.name) return;
  try {
    if (item.type === "folder") await notes.client.notes.renameFolder(item.id, trimmed);
    else await notes.client.notes.rename(item.id, trimmed);
    const siblings = notes.children.get(parentId ?? "") ?? [];
    if (siblings.some((s) => s.id !== item.id && s.type === item.type && s.name === trimmed)) {
      toasts.show({ message: `„${trimmed}“ gibt es in diesem Ordner schon.` });
    }
    await notes.reload();
  } catch (e) {
    fail("Nicht umbenannt", e);
  }
}

export async function move(notes: NotesState, item: NoteTreeItemDto, target: NoteTargetDto) {
  try {
    if (item.type === "folder") await notes.client.notes.moveFolder(item.id, target);
    else await notes.client.notes.move(item.id, target);
    if (target.folderId) await notes.expand(target.folderId);
    await notes.reload();
  } catch (e) {
    fail("Nicht verschoben", e);
  }
}

export async function setKind(notes: NotesState, id: string, kind: NoteKindDto) {
  try {
    await notes.client.notes.setKind(id, kind);
    await notes.reload();
  } catch (e) {
    fail("Typ nicht geändert", e);
  }
}

/** Neuer Ordner „Neuer Ordner" in `parentId`; liefert seine ID (zum Umbenennen). */
export async function newFolder(notes: NotesState, parentId: string | null): Promise<string | null> {
  try {
    const folder = await notes.client.notes.createFolder(parentId, "Neuer Ordner");
    if (parentId) await notes.expand(parentId);
    await notes.reload();
    notes.cursor = `f:${folder.id}`;
    return folder.id;
  } catch (e) {
    fail("Ordner nicht angelegt", e);
    return null;
  }
}
