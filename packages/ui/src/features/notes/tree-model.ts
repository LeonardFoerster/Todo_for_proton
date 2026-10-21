/**
 * Reine Logik des Notiz-Baums (`notes.md` §5.3): geladene Ordnerinhalte + aufgeklappte Ordner →
 * flache Liste sichtbarer Zeilen (für die virtuelle Liste), Nachbarn, Ziele beim Ziehen.
 */
import type { NoteItemDto, NoteTreeItemDto } from "@tasks/core-client";

/** Schlüssel des Wurzelinhalts in `children`. */
export const ROOT = "";

export interface TreeRow {
  /** `f:<id>` bzw. `n:<id>` – eindeutig über Ordner und Notizen. */
  key: string;
  item: NoteTreeItemDto;
  depth: number;
  /** Nur Ordner: aufgeklappt? */
  expanded: boolean;
  /** Ordner, in dem die Zeile liegt (`null` = Wurzel). */
  parentId: string | null;
}

export function itemKey(item: { type: "folder" | "note"; id: string }): string {
  return `${item.type === "folder" ? "f" : "n"}:${item.id}`;
}

export function itemRef(item: NoteTreeItemDto): NoteItemDto {
  return { type: item.type, id: item.id };
}

/** Sichtbare Zeilen: Wurzel, darunter aufgeklappte Ordner mit ihrem (geladenen) Inhalt. */
export function flatten(
  children: ReadonlyMap<string, readonly NoteTreeItemDto[]>,
  expanded: ReadonlySet<string>,
): TreeRow[] {
  const rows: TreeRow[] = [];
  // Während zwei Ordner nacheinander neu laden (Verschieben), kann ein Element kurz in beiden
  // stehen: einmal zeigen genügt.
  const shown = new Set<string>();
  const visit = (parentId: string | null, depth: number, seen: Set<string>) => {
    for (const item of children.get(parentId ?? ROOT) ?? []) {
      const key = itemKey(item);
      if (shown.has(key)) continue;
      shown.add(key);
      const open = item.type === "folder" && expanded.has(item.id);
      rows.push({ key, item, depth, expanded: open, parentId });
      // `seen` schützt vor Zyklen in fehlerhaften Daten (der Core liefert zyklenfrei).
      if (open && !seen.has(item.id)) visit(item.id, depth + 1, new Set(seen).add(item.id));
    }
  };
  visit(null, 0, new Set());
  return rows;
}

/** Zeile des Elternordners von `rows[index]` (für `←`). */
export function parentIndex(rows: readonly TreeRow[], index: number): number {
  const row = rows[index];
  if (!row || row.parentId === null) return -1;
  return rows.findIndex((r) => r.item.type === "folder" && r.item.id === row.parentId);
}

/** Nachbarn unter Geschwistern für „nach oben/unten" (`Alt+↑/↓`): Zielposition oder `null`. */
export function siblingTarget(
  rows: readonly TreeRow[],
  index: number,
  delta: -1 | 1,
): { folderId: string | null; after?: NoteItemDto; before?: NoteItemDto } | null {
  const row = rows[index];
  if (!row) return null;
  const siblings = rows.filter((r) => r.parentId === row.parentId && r.depth === row.depth);
  const i = siblings.findIndex((r) => r.key === row.key);
  const j = i + delta;
  if (j < 0 || j >= siblings.length) return null;
  const other = itemRef(siblings[j].item);
  return delta < 0 ? { folderId: row.parentId, before: other } : { folderId: row.parentId, after: other };
}

/** Wohin eine gezogene Zeile fällt. */
export type DropTarget =
  | { kind: "into"; folderId: string; rowIndex: number }
  | { kind: "between"; folderId: string | null; after?: NoteItemDto; before?: NoteItemDto; line: number };

/**
 * Ziel für einen Zeiger bei `y` (px ab Listenanfang): mittlere Hälfte eines Ordners = hinein,
 * sonst zwischen die Zeilen (Einfügelinie). `moving` und sein Teilbaum sind kein Ziel.
 */
export function dropTarget(
  rows: readonly TreeRow[],
  y: number,
  rowHeight: number,
  moving: string,
): DropTarget | null {
  if (rows.length === 0) return { kind: "between", folderId: null, line: 0 };
  const index = Math.max(0, Math.min(rows.length - 1, Math.floor(y / rowHeight)));
  const row = rows[index];
  const offset = y - index * rowHeight;
  const inside = (r: TreeRow) => isInside(rows, r, moving);
  if (row.item.type === "folder" && offset > rowHeight * 0.25 && offset < rowHeight * 0.75) {
    if (row.key === moving || inside(row)) return null;
    return { kind: "into", folderId: row.item.id, rowIndex: index };
  }
  // Linie über `row` (obere Hälfte) bzw. darunter.
  const below = offset >= rowHeight / 2;
  const anchor = row;
  if (anchor.key === moving || inside(anchor)) return null;
  if (below) {
    // Unter einem aufgeklappten Ordner: als erstes Kind hinein.
    if (anchor.item.type === "folder" && anchor.expanded) {
      const first = rows[index + 1];
      return first && first.parentId === anchor.item.id
        ? { kind: "between", folderId: anchor.item.id, before: itemRef(first.item), line: (index + 1) * rowHeight }
        : { kind: "into", folderId: anchor.item.id, rowIndex: index };
    }
    return { kind: "between", folderId: anchor.parentId, after: itemRef(anchor.item), line: (index + 1) * rowHeight };
  }
  return { kind: "between", folderId: anchor.parentId, before: itemRef(anchor.item), line: index * rowHeight };
}

/** Liegt `row` im Teilbaum der gezogenen Zeile `moving`? */
function isInside(rows: readonly TreeRow[], row: TreeRow, moving: string): boolean {
  let parent = row.parentId;
  const byId = new Map(rows.filter((r) => r.item.type === "folder").map((r) => [r.item.id, r]));
  const seen = new Set<string>();
  while (parent !== null && !seen.has(parent)) {
    if (`f:${parent}` === moving) return true;
    seen.add(parent);
    parent = byId.get(parent)?.parentId ?? null;
  }
  return false;
}

/** „vor 3 Tagen" für den Papierkorb. */
export function ago(iso: string, now = new Date()): string {
  const ms = now.getTime() - Date.parse(iso);
  const minutes = Math.round(ms / 60_000);
  if (minutes < 1) return "gerade eben";
  if (minutes < 60) return `vor ${minutes} Min.`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return hours === 1 ? "vor 1 Stunde" : `vor ${hours} Stunden`;
  const days = Math.round(hours / 24);
  return days === 1 ? "vor 1 Tag" : `vor ${days} Tagen`;
}
