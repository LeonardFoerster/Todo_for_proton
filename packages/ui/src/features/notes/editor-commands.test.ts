import { describe, expect, it } from "vitest";
import {
  continueList,
  heading,
  indent,
  link,
  linkFromPaste,
  outdent,
  rebase,
  replacePreservingCursor,
  stats,
  toggleCheckbox,
  wrap,
  type Edit,
  type TextState,
} from "./editor-commands";

/** Text mit `|` als Cursor bzw. `«`…`»` als Auswahl → Zustand. */
function at(marked: string): TextState {
  const sel = marked.indexOf("«");
  if (sel >= 0) {
    const end = marked.indexOf("»", sel);
    const value = marked.slice(0, sel) + marked.slice(sel + 1, end) + marked.slice(end + 1);
    return { value, start: sel, end: end - 1 };
  }
  const caret = marked.indexOf("|");
  return { value: marked.replace("|", ""), start: caret, end: caret };
}

/** Änderung anwenden und mit Cursor bzw. Auswahl darstellen. */
function show(s: TextState, edit: Edit | null): string | null {
  if (!edit) return null;
  const value = s.value.slice(0, edit.from) + edit.insert + s.value.slice(edit.to);
  if (edit.selStart === edit.selEnd) return value.slice(0, edit.selStart) + "|" + value.slice(edit.selStart);
  return `${value.slice(0, edit.selStart)}«${value.slice(edit.selStart, edit.selEnd)}»${value.slice(edit.selEnd)}`;
}

const run = (marked: string, f: (s: TextState) => Edit | null) => show(at(marked), f(at(marked)));

describe("continueList", () => {
  it.each([
    ["- Milch|", "- Milch\n- |"],
    ["* a|", "* a\n* |"],
    ["  + tief|", "  + tief\n  + |"],
    ["1. eins|", "1. eins\n2. |"],
    ["9) neun|", "9) neun\n10) |"],
    ["- [x] erledigt|", "- [x] erledigt\n- [ ] |"],
    ["> Zitat|", "> Zitat\n> |"],
    ["- |", "|"],
    ["text\n- [ ] |", "text\n|"],
  ])("%j → %j", (input, output) => {
    expect(run(input, continueList)).toBe(output);
  });

  it("only at the end of an item and without selection", () => {
    expect(run("- Mi|lch", continueList)).toBeNull();
    expect(run("Text|", continueList)).toBeNull();
    expect(run("- «Milch»", continueList)).toBeNull();
  });
});

describe("indent / outdent", () => {
  it("inserts at the caret outside lists, indents list items and multi-line selections", () => {
    expect(run("ab|c", (s) => indent(s, "markdown"))).toBe("ab  |c");
    expect(run("ab|c", (s) => indent(s, "text"))).toBe("ab\t|c");
    expect(run("- a|", (s) => indent(s, "markdown"))).toBe("  - a|");
    expect(run("«a\nb»", (s) => indent(s, "markdown"))).toBe("  «a\n  b»");
    expect(run("x\n«a\nb\n»y", (s) => indent(s, "text"))).toBe("x\n\t«a\n\tb\n»y");
  });

  it("outdents line by line", () => {
    expect(run("  - a|", (s) => outdent(s, "markdown"))).toBe("- a|");
    expect(run("«  a\n    b»", (s) => outdent(s, "markdown"))).toBe("«a\n  b»");
    expect(run("\t«a»", (s) => outdent(s, "text"))).toBe("«a»");
    expect(run("a|", (s) => outdent(s, "markdown"))).toBeNull();
  });
});

describe("format", () => {
  it("wraps and unwraps", () => {
    expect(run("a «fett» b", (s) => wrap(s, "**"))).toBe("a **«fett»** b");
    expect(run("a **«fett»** b", (s) => wrap(s, "**"))).toBe("a «fett» b");
    expect(run("a | b", (s) => wrap(s, "_"))).toBe("a _|_ b");
    expect(run("«weg»", (s) => wrap(s, "~~"))).toBe("~~«weg»~~");
  });

  it("links", () => {
    expect(run("«Proton»", link)).toBe("[Proton](|)");
    expect(run("a |", link)).toBe("a [|]()");
    expect(show(at("«Seite»"), linkFromPaste(at("«Seite»"), "https://example.org/a"))).toBe(
      "[Seite](https://example.org/a)|",
    );
    expect(linkFromPaste(at("«Seite»"), "kein link")).toBeNull();
    expect(linkFromPaste(at("|"), "https://example.org")).toBeNull();
  });

  it("sets and removes headings", () => {
    expect(run("Titel|", (s) => heading(s, 1))).toBe("# Titel|");
    expect(run("# Ti|tel", (s) => heading(s, 2))).toBe("## Ti|tel");
    expect(run("### |Titel", (s) => heading(s, 0))).toBe("|Titel");
  });

  it("toggles checkboxes", () => {
    expect(run("- [ ] a|", toggleCheckbox)).toBe("- [x] a|");
    expect(run("- [x] a|", toggleCheckbox)).toBe("- [ ] a|");
    expect(run("- a|", toggleCheckbox)).toBe("- [ ] a|");
    expect(run("  Milch|", toggleCheckbox)).toBe("  - [ ] Milch|");
  });
});

describe("stats and remote replacement", () => {
  it("counts words and characters", () => {
    expect(stats("")).toEqual({ words: 0, chars: 0 });
    expect(stats("Hallo  Welt\n😀 x")).toEqual({ words: 4, chars: 15 });
  });

  it("replaces text keeping the caret where it belongs", () => {
    const el = document.createElement("textarea");
    el.value = "eins\nzwei\ndrei";
    document.body.append(el);
    el.setSelectionRange(12, 12); // in „drei"
    replacePreservingCursor(el, "EINS!\nzwei\ndrei");
    expect(el.value).toBe("EINS!\nzwei\ndrei");
    expect(el.selectionStart).toBe(13);
    el.setSelectionRange(1, 1);
    replacePreservingCursor(el, "EINS!\nzwei\ndrei\nvier");
    expect(el.selectionStart).toBe(1);
    el.remove();
  });
});

describe("rebase", () => {
  it("moves own typing onto the merged text when the regions are apart", () => {
    const sent = "eins\nzwei\ndrei";
    expect(rebase(sent, "eins\nzwei\ndrei!!", "EINS\nzwei\ndrei")).toBe("EINS\nzwei\ndrei!!");
    expect(rebase(sent, "eins?\nzwei\ndrei", "eins\nzwei\nDREI")).toBe("eins?\nzwei\nDREI");
    expect(rebase(sent, "eins\nzwei\ndrei", "x")).toBe("x");
    expect(rebase(sent, "eins\nzweiX\ndrei", "eins\nzweiY\ndrei")).toBeNull();
  });
});
