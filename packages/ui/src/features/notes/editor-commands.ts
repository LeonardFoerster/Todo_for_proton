/**
 * Editor-Hilfen (`notes.md` §6.4) als reine Funktionen: aus Text + Auswahl wird eine Änderung
 * `{ from, to, insert }` mit neuer Auswahl. Angewendet über `insertText`, damit das native
 * Rückgängig (`Strg+Z`) weiter funktioniert.
 */

export interface TextState {
  value: string;
  start: number;
  end: number;
}

export interface Edit {
  /** Bereich, der durch `insert` ersetzt wird. */
  from: number;
  to: number;
  insert: string;
  /** Auswahl danach. */
  selStart: number;
  selEnd: number;
}

export type Kind = "markdown" | "text";

/** Einrückung je Typ: `.md` zwei Leerzeichen, `.txt` ein Tabulator. */
export function indentUnit(kind: Kind): string {
  return kind === "markdown" ? "  " : "\t";
}

export function lineStart(value: string, pos: number): number {
  return value.lastIndexOf("\n", pos - 1) + 1;
}

export function lineEnd(value: string, pos: number): number {
  const i = value.indexOf("\n", pos);
  return i < 0 ? value.length : i;
}

/** Zeilenblock, den die Auswahl berührt (eine am Zeilenanfang endende Auswahl zählt die Zeile nicht). */
function block(s: TextState): { from: number; to: number } {
  const lastPos = s.end > s.start && s.value[s.end - 1] === "\n" ? s.end - 1 : s.end;
  return { from: lineStart(s.value, s.start), to: lineEnd(s.value, lastPos) };
}

const LIST = /^(\s*)([-*+]|\d+[.)])(\s+)(\[[ xX]\]\s+)?(.*)$/;
const QUOTE = /^(\s*)(>\s?)(.*)$/;

/** Ist die Zeile mit dem Cursor ein Listenpunkt oder Zitat? */
export function inListItem(s: TextState): boolean {
  const line = s.value.slice(lineStart(s.value, s.start), lineEnd(s.value, s.start));
  return LIST.test(line) || QUOTE.test(line);
}

/**
 * `Enter` am Ende eines Listenpunkts setzt die Liste fort (`- `, `* `, `+ `, `1.` → `2.`,
 * `- [ ] `, `> `); ein leerer Punkt beendet sie. `null` = normales Enter.
 */
export function continueList(s: TextState): Edit | null {
  if (s.start !== s.end) return null;
  const from = lineStart(s.value, s.start);
  const to = lineEnd(s.value, s.start);
  if (s.start !== to) return null;
  const line = s.value.slice(from, to);
  const list = LIST.exec(line);
  if (list) {
    const [, indent, marker, gap, box, rest] = list;
    if (rest.trim() === "") return { from, to, insert: "", selStart: from, selEnd: from };
    const number = /^(\d+)([.)])$/.exec(marker);
    const next = number ? `${Number(number[1]) + 1}${number[2]}` : marker;
    const insert = `\n${indent}${next}${gap}${box ? "[ ] " : ""}`;
    return { from: to, to, insert, selStart: to + insert.length, selEnd: to + insert.length };
  }
  const quote = QUOTE.exec(line);
  if (quote) {
    const [, indent, marker, rest] = quote;
    if (rest.trim() === "") return { from, to, insert: "", selStart: from, selEnd: from };
    const insert = `\n${indent}${marker.endsWith(" ") ? marker : `${marker} `}`;
    return { from: to, to, insert, selStart: to + insert.length, selEnd: to + insert.length };
  }
  return null;
}

/** `Tab`: Auswahl über Zeilen bzw. Listenpunkt zeilenweise einrücken, sonst am Cursor einfügen. */
export function indent(s: TextState, kind: Kind): Edit {
  const unit = indentUnit(kind);
  const b = block(s);
  const multi = s.value.slice(b.from, b.to).includes("\n");
  if (s.start === s.end && !multi && !inListItem(s)) {
    return { from: s.start, to: s.end, insert: unit, selStart: s.start + unit.length, selEnd: s.start + unit.length };
  }
  const lines = s.value.slice(b.from, b.to).split("\n");
  const insert = lines.map((l) => unit + l).join("\n");
  return {
    from: b.from,
    to: b.to,
    insert,
    selStart: s.start + unit.length,
    selEnd: s.end + unit.length * lines.length,
  };
}

/** `Umschalt+Tab`: Auswahl zeilenweise ausrücken (ein Tab bzw. bis zu zwei/vier Leerzeichen). */
export function outdent(s: TextState, kind: Kind): Edit | null {
  const max = kind === "markdown" ? 2 : 4;
  const b = block(s);
  const lines = s.value.slice(b.from, b.to).split("\n");
  const removed = lines.map((l) => (l.startsWith("\t") ? 1 : Math.min(max, l.length - l.trimStart().length)));
  if (removed.every((n) => n === 0)) return null;
  const insert = lines.map((l, i) => l.slice(removed[i])).join("\n");
  const first = Math.min(removed[0], s.start - b.from);
  const total = removed.reduce((a, n) => a + n, 0);
  return { from: b.from, to: b.to, insert, selStart: s.start - first, selEnd: Math.max(b.from, s.end - total) };
}

/** Fett/Kursiv/Durchgestrichen: Auswahl einschließen bzw. – schon eingeschlossen – lösen. */
export function wrap(s: TextState, marker: string): Edit {
  const m = marker.length;
  const before = s.value.slice(s.start - m, s.start);
  const after = s.value.slice(s.end, s.end + m);
  if (before === marker && after === marker) {
    const inner = s.value.slice(s.start, s.end);
    return { from: s.start - m, to: s.end + m, insert: inner, selStart: s.start - m, selEnd: s.end - m };
  }
  const inner = s.value.slice(s.start, s.end);
  return {
    from: s.start,
    to: s.end,
    insert: `${marker}${inner}${marker}`,
    selStart: s.start + m,
    selEnd: s.end + m,
  };
}

/** `Strg+L`: `[Auswahl](|)` – Cursor in der Klammer für die Adresse (ohne Auswahl im Text). */
export function link(s: TextState): Edit {
  const text = s.value.slice(s.start, s.end);
  const insert = `[${text}]()`;
  const caret = text ? s.start + text.length + 3 : s.start + 1;
  return { from: s.start, to: s.end, insert, selStart: caret, selEnd: caret };
}

const URL = /^(https?:\/\/|mailto:)\S+$/i;

/** Adresse über markiertem Text einfügen → Link; sonst `null` (normales Einfügen). */
export function linkFromPaste(s: TextState, pasted: string): Edit | null {
  const url = pasted.trim();
  if (s.start === s.end || !URL.test(url)) return null;
  const text = s.value.slice(s.start, s.end);
  if (text.includes("\n")) return null;
  const insert = `[${text}](${url})`;
  return { from: s.start, to: s.end, insert, selStart: s.start + insert.length, selEnd: s.start + insert.length };
}

const HEADING = /^#{1,6}\s+/;

/** `Strg+Alt+1…3` setzt die Überschrift der Zeile(n), `Strg+Alt+0` macht einen Absatz daraus. */
export function heading(s: TextState, level: 0 | 1 | 2 | 3): Edit {
  const b = block(s);
  const lines = s.value.slice(b.from, b.to).split("\n");
  const prefix = level > 0 ? `${"#".repeat(level)} ` : "";
  const next = lines.map((l) => prefix + l.replace(HEADING, ""));
  const insert = next.join("\n");
  const delta = next[0].length - lines[0].length;
  return {
    from: b.from,
    to: b.to,
    insert,
    selStart: Math.max(b.from, s.start + delta),
    selEnd: Math.max(b.from, s.end + insert.length - (b.to - b.from)),
  };
}

/** `Strg+Umschalt+X`: `- [ ]` ↔ `- [x]`; ein Listenpunkt bzw. eine Zeile wird zum Checklistenpunkt. */
export function toggleCheckbox(s: TextState): Edit {
  const b = block(s);
  const lines = s.value.slice(b.from, b.to).split("\n");
  const next = lines.map((l) => {
    const m = LIST.exec(l);
    if (m) {
      const [, ind, marker, gap, box, rest] = m;
      if (!box) return `${ind}${marker}${gap}[ ] ${rest}`;
      return `${ind}${marker}${gap}${/\[ \]/.test(box) ? "[x]" : "[ ]"} ${rest}`;
    }
    const lead = l.length - l.trimStart().length;
    return `${l.slice(0, lead)}- [ ] ${l.slice(lead)}`;
  });
  const insert = next.join("\n");
  const delta = next[0].length - lines[0].length;
  return {
    from: b.from,
    to: b.to,
    insert,
    selStart: s.start + delta,
    selEnd: s.end + insert.length - (b.to - b.from),
  };
}

/** Wörter und Zeichen (Unicode-Zeichen, nicht UTF-16-Einheiten) für die Statuszeile. */
export function stats(text: string): { words: number; chars: number } {
  let words = 0;
  let inWord = false;
  let surrogates = 0;
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (code >= 0xdc00 && code <= 0xdfff) surrogates++;
    const space = code === 32 || code === 10 || code === 9 || code === 13 || code === 0xa0;
    if (!space && !inWord) words++;
    inWord = !space;
  }
  return { words, chars: text.length - surrogates };
}

/**
 * Wendet eine Änderung auf ein `<textarea>` an: über `insertText` (natives Rückgängig), wo der
 * Browser das nicht kann über `setRangeText` mit `input`-Ereignis.
 */
export function applyEdit(el: HTMLTextAreaElement, edit: Edit) {
  el.focus();
  const length = el.value.length - (edit.to - edit.from) + edit.insert.length;
  el.setSelectionRange(edit.from, edit.to);
  let native = false;
  if (typeof document.execCommand === "function" && (edit.from !== edit.to || edit.insert !== "")) {
    try {
      native =
        edit.insert === "" ? document.execCommand("delete") : document.execCommand("insertText", false, edit.insert);
    } catch {
      native = false;
    }
  }
  // Nur prüfen, was sich ändern sollte (keine O(n)-Arbeit pro Taste).
  if (!native || el.value.length !== length || !el.value.startsWith(edit.insert, edit.from)) {
    if (native) return; // Browser hat anders eingefügt (z. B. Autokorrektur) – nicht doppelt ändern.
    el.setRangeText(edit.insert, edit.from, edit.to, "end");
    el.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: edit.insert }));
  }
  el.setSelectionRange(edit.selStart, edit.selEnd);
}

/**
 * Ersetzt den Inhalt durch `next`, ohne den Cursor zu verlieren (`notes.md` §6.3): gemeinsamen
 * Anfang und Ende bestimmen, nur den Bereich dazwischen tauschen und Auswahl/Scroll verschieben.
 */
export function replacePreservingCursor(el: HTMLTextAreaElement, next: string) {
  const old = el.value;
  if (old === next) return;
  let prefix = 0;
  const max = Math.min(old.length, next.length);
  while (prefix < max && old.charCodeAt(prefix) === next.charCodeAt(prefix)) prefix++;
  let suffix = 0;
  while (
    suffix < max - prefix &&
    old.charCodeAt(old.length - 1 - suffix) === next.charCodeAt(next.length - 1 - suffix)
  ) {
    suffix++;
  }
  const { selectionStart: start, selectionEnd: end, scrollTop } = el;
  const oldTo = old.length - suffix;
  const insert = next.slice(prefix, next.length - suffix);
  const shift = (pos: number) => (pos <= prefix ? pos : pos >= oldTo ? pos + insert.length - (oldTo - prefix) : prefix + insert.length);
  el.setRangeText(insert, prefix, oldTo, "preserve");
  el.setSelectionRange(shift(start), shift(end));
  el.scrollTop = scrollTop;
}

/**
 * Während ein Speichern lief, wurde weiter getippt (`sent` → `typed`), und der Core lieferte
 * einen gemergten Text (`sent` → `merged`, Remote-Änderung). Überträgt die eigene Änderung als
 * einen Bereich auf `merged`; `null`, wenn sich beide Änderungen überschneiden.
 */
export function rebase(sent: string, typed: string, merged: string): string | null {
  if (typed === sent) return merged;
  const region = (a: string, b: string) => {
    let p = 0;
    const max = Math.min(a.length, b.length);
    while (p < max && a.charCodeAt(p) === b.charCodeAt(p)) p++;
    let q = 0;
    while (q < max - p && a.charCodeAt(a.length - 1 - q) === b.charCodeAt(b.length - 1 - q)) q++;
    return { from: p, to: a.length - q, insert: b.slice(p, b.length - q) };
  };
  const mine = region(sent, typed);
  const theirs = region(sent, merged);
  if (mine.to < theirs.from) {
    return merged.slice(0, mine.from) + mine.insert + merged.slice(mine.to);
  }
  if (theirs.to < mine.from) {
    const shift = theirs.insert.length - (theirs.to - theirs.from);
    return merged.slice(0, mine.from + shift) + mine.insert + merged.slice(mine.to + shift);
  }
  return null;
}
