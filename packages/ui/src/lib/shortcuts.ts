/**
 * Zentrale Tastaturkürzel (ui-guidelines §5): eine Registry, ein `keydown`-Listener – kein
 * verstreutes `keydown` in Komponenten. Kombinationen: `ctrl+n`, `alt+arrowup`, `space`;
 * Folgen mit Leerzeichen: `g i` (zweite Taste innerhalb von 1,5 s).
 */

export interface Shortcut {
  /** Eine oder mehrere Belegungen, z. B. `["arrowdown", "j"]`. */
  keys: string[];
  /** Für Übersicht/Tooltips, z. B. „Nächste Task“. */
  description: string;
  /** Anzeige der Belegung, z. B. „↓ / J“ (sonst aus `keys`). */
  label?: string;
  group?: string;
  /** Auch in Textfeldern auslösen (z. B. Esc, Strg+N). */
  inInputs?: boolean;
  /** Nur aktiv, wenn das `true` liefert. */
  when?: () => boolean;
  /** Auch aktiv, während ein modaler Zustand sperrt (z. B. Esc, ?). */
  inModal?: boolean;
  run: (e: KeyboardEvent) => void;
}

const SEQUENCE_TIMEOUT_MS = 1500;

/** Normalisiert ein Ereignis zu `ctrl+alt+shift+meta+taste` (Taste klein, Leertaste = `space`). */
export function comboOf(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): string {
  let key = e.key === " " ? "space" : e.key.toLowerCase();
  if (key === "del") key = "delete";
  const mods = [e.ctrlKey && "ctrl", e.altKey && "alt", e.metaKey && "meta"].filter(Boolean) as string[];
  // Shift bei Nicht-Zeichen-Tasten und Buchstaben (`shift+t`); bei Satzzeichen nicht – „?“ ist je
  // nach Layout schon „shift+ß“ o. ä.
  if (e.shiftKey && (key.length > 1 || /^\p{L}$/u.test(key))) mods.push("shift");
  return [...mods, key].join("+");
}

function isTextTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (target instanceof HTMLInputElement) {
    return !["checkbox", "radio", "button", "submit", "reset", "range", "color"].includes(target.type);
  }
  return false;
}

/** Leertaste/Enter auf echten Bedienelementen bleiben nativ (Button drücken, Link folgen). */
function isNativeActivation(e: KeyboardEvent): boolean {
  if (e.key !== " " && e.key !== "Enter") return false;
  const t = e.target;
  return t instanceof HTMLElement && ["BUTTON", "A", "SUMMARY"].includes(t.tagName);
}

export class ShortcutRegistry {
  private shortcuts: Shortcut[] = [];
  private pending: { first: string; at: number } | null = null;
  private modal: () => boolean = () => false;

  /** Solange `isModal()` gilt (Dialog offen), laufen nur Kürzel mit `inModal`. */
  setModal(isModal: () => boolean): void {
    this.modal = isModal;
  }

  /** Registriert Kürzel; gibt die Abmeldung zurück. */
  register(...shortcuts: Shortcut[]): () => void {
    this.shortcuts.push(...shortcuts);
    return () => {
      this.shortcuts = this.shortcuts.filter((s) => !shortcuts.includes(s));
    };
  }

  list(): readonly Shortcut[] {
    return this.shortcuts;
  }

  /** Verarbeitet ein `keydown`; `true`, wenn ein Kürzel ausgelöst wurde. */
  handle(e: KeyboardEvent, now = Date.now()): boolean {
    if (e.defaultPrevented || e.isComposing || ["Control", "Alt", "Shift", "Meta"].includes(e.key)) return false;
    const combo = comboOf(e);
    const inText = isTextTarget(e.target);
    const pending = this.pending && now - this.pending.at < SEQUENCE_TIMEOUT_MS ? this.pending.first : null;
    this.pending = null;

    const modal = this.modal();
    const candidates = this.shortcuts.filter(
      (s) =>
        (!modal || s.inModal) &&
        (!inText || s.inInputs) &&
        (!s.when || s.when()) &&
        !(isNativeActivation(e) && !s.inInputs),
    );
    const match =
      (pending && candidates.find((s) => s.keys.includes(`${pending} ${combo}`))) ||
      candidates.find((s) => s.keys.includes(combo));
    if (match) {
      e.preventDefault();
      match.run(e);
      return true;
    }
    // Erste Taste einer Folge merken (z. B. „g“).
    if (!inText && candidates.some((s) => s.keys.some((k) => k.startsWith(`${combo} `)))) {
      this.pending = { first: combo, at: now };
      e.preventDefault();
      return true;
    }
    return false;
  }

  /** Hängt den Listener an `target` (Standard: `window`); gibt die Abmeldung zurück. */
  attach(target: Window | HTMLElement = window): () => void {
    const listener = (e: Event) => void this.handle(e as KeyboardEvent);
    target.addEventListener("keydown", listener);
    return () => target.removeEventListener("keydown", listener);
  }
}

/** Anzeige einer Belegung: `ctrl+n` → „Strg+N“, `arrowup` → „↑“, `g i` → „G dann I“. */
export function formatKeys(keys: string): string {
  const names: Record<string, string> = {
    ctrl: "Strg",
    alt: "Alt",
    shift: "Umschalt",
    meta: "Super",
    arrowup: "↑",
    arrowdown: "↓",
    arrowleft: "←",
    arrowright: "→",
    space: "Leertaste",
    enter: "Enter",
    escape: "Esc",
    delete: "Entf",
  };
  return keys
    .split(" ")
    .map((step) =>
      step
        .split("+")
        .map((k) => names[k] ?? k.toUpperCase())
        .join("+"),
    )
    .join(" dann ");
}
