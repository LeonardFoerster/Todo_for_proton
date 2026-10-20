/**
 * Einstellungen und gemerkter Zustand des Modus „Notes" pro Gerät (`notes.md` §5.2, §6.6) –
 * außerhalb des Notes-Chunks, weil auch die Einstellungen sie lesen. Nie synchronisiert.
 */
import type { NoteKindDto } from "@tasks/core-client";
import { readChoice, readFlag, writeChoice, writeFlag } from "./preferences";

export const DEFAULT_KIND_KEY = "notes.default-kind";
export const LIMIT_WIDTH_KEY = "notes.limit-width";
export const ZEN_KEY = "notes.zen";
export const VIEW_KEY = "notes.view";
const VIEW_ENTRIES = 50;

/** Standardtyp neuer Notizen. */
export function defaultKind(): NoteKindDto {
  return readChoice(DEFAULT_KIND_KEY, ["markdown", "text"] as const, "markdown");
}

export function setDefaultKind(kind: NoteKindDto) {
  writeChoice(DEFAULT_KIND_KEY, kind);
}

/** Textspalte auf 72 Zeichen begrenzen (Standard an). */
export function limitWidth(): boolean {
  return readFlag(LIMIT_WIDTH_KEY, true);
}

export function setLimitWidth(on: boolean) {
  writeFlag(LIMIT_WIDTH_KEY, on);
  dispatchEvent(new CustomEvent("notes-preferences"));
}

export function readZen(): boolean {
  return readFlag(ZEN_KEY);
}

export function writeZen(on: boolean) {
  writeFlag(ZEN_KEY, on);
}

/** Je Notiz: Vorschau an, Auswahl, Scroll – zuletzt benutzte zuerst, höchstens 50. */
export interface NoteView {
  preview?: boolean;
  start?: number;
  end?: number;
  scroll?: number;
}

function readViews(): [string, NoteView][] {
  try {
    const raw = localStorage.getItem(VIEW_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? (parsed as [string, NoteView][]) : [];
  } catch {
    return [];
  }
}

export function readNoteView(id: string): NoteView {
  return readViews().find(([key]) => key === id)?.[1] ?? {};
}

export function writeNoteView(id: string, view: NoteView) {
  const rest = readViews().filter(([key]) => key !== id);
  try {
    localStorage.setItem(VIEW_KEY, JSON.stringify([[id, view], ...rest].slice(0, VIEW_ENTRIES)));
  } catch {
    // egal – gilt dann nur bis zum Neustart
  }
}
