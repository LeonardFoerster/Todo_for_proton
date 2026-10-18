import type { TaskSummaryDto } from "@tasks/core-client";
import { SvelteSet } from "svelte/reactivity";

/** Zeilenreihenfolge für Bereiche; ein `string[]` passt, `TaskListState.order()` hat O(1)-`indexOf`. */
export interface RowOrder {
  readonly length: number;
  at(index: number): string | undefined;
  indexOf(id: string): number;
}

/**
 * Mehrfachauswahl einer Ansicht (P0). Getrennt vom *Cursor* (fokussierte Zeile, Detail): Die
 * Auswahl ist eine Menge von IDs plus ein Anker für Bereiche (Umschalt+Klick).
 *
 * Kosten: Umschalten O(1), Bereich O(Bereich) und `extend` O(1) mit einer `RowOrder` mit
 * O(1)-`indexOf`; nur „Alle auswählen" und `prune` sind O(n). Zeilen fragen `has(id)` – kein
 * abgeleitetes Array.
 */
export class SelectionState {
  /** Auswahlmodus an (Kästchen sichtbar, Klick schaltet um statt zu öffnen). */
  active = $state(false);
  readonly ids = new SvelteSet<string>();
  /** Ausgangspunkt für Bereiche; `null` = Cursor-Zeile. */
  anchor = $state<string | null>(null);
  count = $derived(this.ids.size);

  has(id: string): boolean {
    return this.ids.has(id);
  }

  /** Modus an, optional mit Startauswahl (z. B. Cursor-Zeile bei Strg+Klick). */
  enter(initial: readonly string[] = []): void {
    this.active = true;
    for (const id of initial) this.ids.add(id);
    if (initial.length > 0) this.anchor = initial[initial.length - 1];
  }

  exit(): void {
    this.active = false;
    this.clear();
  }

  /** Eine Zeile umschalten; sie wird Anker für folgende Bereiche. */
  toggle(id: string): void {
    if (this.ids.has(id)) this.ids.delete(id);
    else this.ids.add(id);
    this.anchor = id;
  }

  /**
   * Bereich Anker → `toId` (beide eingeschlossen) in der Reihenfolge `order` **hinzufügen**.
   * Ohne (sichtbaren) Anker gilt `cursor`, sonst `toId` allein; der Startpunkt wird Anker.
   */
  range(toId: string, order: RowOrder, cursor: string | null = null): void {
    const to = order.indexOf(toId);
    if (to < 0) return;
    let from = this.anchor === null ? -1 : order.indexOf(this.anchor);
    if (from < 0) from = cursor === null ? -1 : order.indexOf(cursor);
    if (from < 0) from = to;
    this.anchor = order.at(from)!;
    const [lo, hi] = from <= to ? [from, to] : [to, from];
    for (let i = lo; i <= hi; i++) this.ids.add(order.at(i)!);
  }

  /**
   * Umschalt+↑/↓: Cursor um `delta` bewegen und Cursor- wie Zielzeile auswählen. Liefert die neue
   * Cursor-ID (am Rand die bisherige).
   */
  extend(delta: number, cursor: string | null, order: RowOrder): string | null {
    if (order.length === 0) return cursor;
    const from = cursor === null ? -1 : order.indexOf(cursor);
    if (from < 0) {
      const start = order.at(delta > 0 ? 0 : order.length - 1)!;
      this.ids.add(start);
      this.anchor ??= start;
      return start;
    }
    const [current, next] = [order.at(from)!, order.at(Math.max(0, Math.min(order.length - 1, from + delta)))!];
    this.anchor ??= current;
    this.ids.add(current);
    this.ids.add(next);
    return next;
  }

  /** Genau diese IDs (Alle auswählen). */
  selectAll(ids: readonly string[]): void {
    this.ids.clear();
    for (const id of ids) this.ids.add(id);
  }

  clear(): void {
    this.ids.clear();
    this.anchor = null;
  }

  /** Entfernt IDs, die nicht mehr in der Ansicht sind (Sync, Löschen, Verschieben). */
  prune(present: ReadonlySet<string>): void {
    for (const id of [...this.ids]) {
      if (!present.has(id)) this.ids.delete(id);
    }
    if (this.anchor !== null && !present.has(this.anchor)) this.anchor = null;
  }

  /**
   * Für die Aktionsleiste: Sind offene bzw. erledigte Tasks in der Auswahl? Betrachtet nur
   * geladene Zeilen (`lookup`); bricht ab, sobald beides feststeht.
   */
  flags(lookup: (id: string) => TaskSummaryDto | undefined): { hasOpen: boolean; hasDone: boolean } {
    let hasOpen = false;
    let hasDone = false;
    for (const id of this.ids) {
      const status = lookup(id)?.status;
      if (status === "open") hasOpen = true;
      else if (status === "done") hasDone = true;
      if (hasOpen && hasDone) break;
    }
    return { hasOpen, hasDone };
  }
}
