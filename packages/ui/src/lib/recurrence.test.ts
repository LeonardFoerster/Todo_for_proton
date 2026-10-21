import type { RecurrenceDto } from "@tasks/core-client";
import { describe, expect, it } from "vitest";
import { describeRule, firstOnOrAfter, presetOf, rule, ruleChipText } from "./recurrence";

describe("recurrence (N4)", () => {
  it.each<[RecurrenceDto, string, string]>([
    [rule("daily"), "täglich", "Täglich"],
    [rule("weekly", { weekdays: ["th", "mo"] }), "wöchentlich, Mo und Do", "Wöchentlich"],
    [rule("weekly", { weekdays: ["mo", "tu", "we", "th", "fr"] }), "werktags", "Werktags"],
    [rule("weekly", { interval: 2 }), "alle 2 Wochen", "Alle 2 Wochen"],
    [rule("monthly", { monthDay: 15 }), "monatlich am 15.", "Monatlich"],
    [rule("monthly", { monthDay: 31 }), "monatlich am Monatsletzten", "Monatlich"],
    [rule("daily", { interval: 3, anchor: "completion", until: "2026-12-20" }), "alle 3 Tage nach Erledigung, bis 20.12.2026", "Alle 3 Tage"],
  ])("%j", (r, text, chip) => {
    expect(describeRule(r)).toBe(text);
    expect(ruleChipText(r)).toBe(chip);
  });

  it("presets and the first occurrence", () => {
    const due = { kind: "date", date: "2026-10-19" } as const; // Montag
    expect(presetOf(rule("weekly", { weekdays: ["mo"] }), due)).toBe("weekly");
    expect(presetOf(rule("weekly", { weekdays: ["tu"] }), due)).toBeNull();
    expect(presetOf(rule("monthly", { monthDay: 19 }), due)).toBe("monthly");
    expect(presetOf(rule("daily", { interval: 2 }), due)).toBeNull();
    const sunday = new Date(2026, 9, 18, 10);
    expect(firstOnOrAfter(rule("weekly", { weekdays: ["we"] }), sunday)).toEqual(new Date(2026, 9, 21));
    expect(firstOnOrAfter(rule("monthly", { monthDay: 15 }), sunday)).toEqual(new Date(2026, 10, 15));
    expect(firstOnOrAfter(rule("monthly", { monthDay: 31 }), new Date(2026, 10, 2))).toEqual(new Date(2026, 10, 30));
    expect(firstOnOrAfter(rule("daily"), sunday)).toEqual(new Date(2026, 9, 18));
  });
});
