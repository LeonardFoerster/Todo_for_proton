import { describe, expect, it } from "vitest";
import { parseQuickAdd } from "./parse";

const now = new Date(2026, 9, 18, 10);

describe("parseQuickAdd", () => {
  it("extracts simple day words anywhere", () => {
    expect(parseQuickAdd("Zahnarzt anrufen morgen", now)).toEqual({
      title: "Zahnarzt anrufen",
      due: { kind: "date", date: "2026-10-19" },
      dueWord: "morgen",
    });
    expect(parseQuickAdd("Heute Milch kaufen", now).due).toEqual({ kind: "date", date: "2026-10-18" });
    expect(parseQuickAdd("Bericht übermorgen fertig", now)).toMatchObject({ title: "Bericht fertig", dueWord: "übermorgen" });
  });

  it("ignores partial words and keeps lone words as title", () => {
    expect(parseQuickAdd("Morgenroutine planen", now).due).toBeNull();
    expect(parseQuickAdd("Heutemorgen", now).due).toBeNull();
    expect(parseQuickAdd("morgen", now)).toEqual({ title: "morgen", due: null, dueWord: null });
  });

  it("uses the last day word", () => {
    expect(parseQuickAdd("heute oder morgen anrufen", now)).toMatchObject({ title: "heute oder anrufen", dueWord: "morgen" });
  });
});
