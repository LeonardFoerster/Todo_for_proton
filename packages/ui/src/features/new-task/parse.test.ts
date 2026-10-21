import type { TaskListDto } from "@tasks/core-client";
import { describe, expect, it } from "vitest";
import { parseDue, parseNewTask, recognizedMarks } from "./parse";

/** Sonntag, 18.10.2026, 10:00 Uhr. */
const now = new Date(2026, 9, 18, 10);
const parse = (input: string, lists?: TaskListDto[]) => parseNewTask(input, { now, lists, tz: "Europe/Berlin" });
const date = (d: string) => ({ kind: "date", date: d });
const at = (y: number, m: number, d: number, h: number, min = 0) => ({
  kind: "dateTime",
  at: new Date(y, m - 1, d, h, min).toISOString(),
  tz: "Europe/Berlin",
});

const list = (id: string, name: string, isInbox = false): TaskListDto => ({ id, name, isInbox, sortKey: id });
const lists = [list("in", "", true), list("a", "Uni – Analysis"), list("b", "Uni – Bio"), list("e", "Einkauf")];

describe("parseNewTask: Wiederholung (N4-05)", () => {
  const r = (freq: string, extra: object = {}) => ({ freq, interval: 1, weekdays: [], anchor: "schedule", ...extra });
  it.each([
    ["Blumen gießen täglich", r("daily"), date("2026-10-18")],
    ["Blumen gießen jeden Tag", r("daily"), date("2026-10-18")],
    ["Müll wöchentlich", r("weekly"), date("2026-10-18")],
    ["Müll jede Woche", r("weekly"), date("2026-10-18")],
    ["Sport jeden Mo", r("weekly", { weekdays: ["mo"] }), date("2026-10-19")],
    ["Sport jeden Mo und Do", r("weekly", { weekdays: ["mo", "th"] }), date("2026-10-19")],
    ["Sport jeden Mo, Do", r("weekly", { weekdays: ["mo", "th"] }), date("2026-10-19")],
    ["Sport jeden Sa und So", r("weekly", { weekdays: ["sa", "su"] }), date("2026-10-18")],
    ["Bad putzen alle 2 Wochen", r("weekly", { interval: 2 }), date("2026-10-18")],
    ["Wasser alle 3 Tage", r("daily", { interval: 3 }), date("2026-10-18")],
    ["Miete monatlich", r("monthly"), date("2026-10-18")],
    ["Miete jeden Monat", r("monthly"), date("2026-10-18")],
    ["Miete jeden 15.", r("monthly", { monthDay: 15 }), date("2026-11-15")],
    ["Miete jeden 31.", r("monthly", { monthDay: 31 }), date("2026-10-31")],
    ["Steuer jährlich", r("yearly"), date("2026-10-18")],
    ["Standup werktags", r("weekly", { weekdays: ["mo", "tu", "we", "th", "fr"] }), date("2026-10-19")],
  ])("%s", (input, recurrence, due) => {
    const p = parse(input);
    expect(p.recurrence).toEqual(recurrence);
    expect(p.due).toEqual(due);
    expect(p.title).toBe(input.split(" ").slice(0, input.startsWith("Blumen") || input.startsWith("Bad") ? 2 : 1).join(" "));
  });

  it("anchor, end, time and an explicit day", () => {
    expect(parse("Haare schneiden alle 6 Wochen nach Erledigung")).toMatchObject({
      title: "Haare schneiden",
      recurrence: r("weekly", { interval: 6, anchor: "completion" }),
    });
    expect(parse("Kurs jeden Di bis 20.12.")).toMatchObject({
      title: "Kurs",
      recurrence: r("weekly", { weekdays: ["tu"], until: "2026-12-20" }),
      due: date("2026-10-20"),
    });
    // Uhrzeit schon vorbei → erstes Vorkommen ab morgen; „um“ fällt weg.
    expect(parse("Tabletten täglich um 8:00")).toMatchObject({ title: "Tabletten", due: at(2026, 10, 19, 8) });
    expect(parse("Tabletten täglich 20:00").due).toEqual(at(2026, 10, 18, 20));
    expect(parse("Lauf jeden Mo 7:00").due).toEqual(at(2026, 10, 19, 7));
    // Ein Tag im Text ist der Start.
    expect(parse("Review wöchentlich ab Fr").due).toEqual(date("2026-10-23"));
  });

  it("keeps words without a rule in the title and marks the rule", () => {
    expect(parse("jeden fragen")).toMatchObject({ title: "jeden fragen", recurrence: null });
    expect(parse("alle Fenster putzen")).toMatchObject({ title: "alle Fenster putzen", recurrence: null });
    expect(parse("täglich")).toMatchObject({ title: "täglich", recurrence: null });
    const input = "Sport jeden Mo und Do 7:00";
    const marks = recognizedMarks(input, { now });
    expect(marks.map((m) => [input.slice(m.start, m.end), m.kind])).toEqual([["jeden Mo und Do", "repeat"], ["7:00", "due"]]);
  });
});

describe("parseNewTask: Tags (N8-03)", () => {
  it("collects every #tag, drops them from the title and marks them", () => {
    expect(parse("Blatt 3 #uni #Analysis morgen")).toMatchObject({
      title: "Blatt 3",
      tags: ["uni", "Analysis"],
      due: date("2026-10-19"),
    });
    expect(parse("Lesen #uni, #UNI.").tags).toEqual(["uni"]);
    expect(parse("Fix #12 bauen")).toMatchObject({ title: "Fix #12 bauen" });
    expect(parse("Fix #12 bauen").tags).toBeUndefined();
    expect(parse("C# lernen").tags).toBeUndefined();
    const input = "Blatt #uni !1";
    expect(recognizedMarks(input, { now }).map((m) => [input.slice(m.start, m.end), m.kind])).toEqual([
      ["#uni", "tag"],
      ["!1", "priority"],
    ]);
  });
});

describe("parseNewTask", () => {
  it("extracts simple day words anywhere", () => {
    expect(parse("Zahnarzt anrufen morgen")).toEqual({
      title: "Zahnarzt anrufen",
      due: date("2026-10-19"),
      priority: null,
      listId: null,
      recurrence: null,
    });
    expect(parse("Heute Milch kaufen").due).toEqual(date("2026-10-18"));
    expect(parse("Bericht übermorgen fertig")).toMatchObject({ title: "Bericht fertig", due: date("2026-10-20") });
  });

  it("ignores partial words and keeps lone words as title", () => {
    expect(parse("Morgenroutine planen").due).toBeNull();
    expect(parse("Heutemorgen").due).toBeNull();
    expect(parse("morgen")).toEqual({ title: "morgen", due: null, priority: null, listId: null, recurrence: null });
    expect(parse("Fr !1")).toMatchObject({ title: "Fr !1", due: null, priority: null });
  });

  it("uses the last day and removes only that one", () => {
    expect(parse("heute oder morgen anrufen")).toMatchObject({ title: "heute oder anrufen", due: date("2026-10-19") });
  });

  it("knows weekdays (next one, never today) but not the filler word „so“", () => {
    expect(parse("Blatt abgeben Fr")).toMatchObject({ title: "Blatt abgeben", due: date("2026-10-23") });
    expect(parse("Blatt abgeben freitag").due).toEqual(date("2026-10-23"));
    expect(parse("Sport Sa").due).toEqual(date("2026-10-24"));
    expect(parse("Mo. Mensa").due).toEqual(date("2026-10-19"));
    expect(parse("so schnell wie möglich")).toMatchObject({ title: "so schnell wie möglich", due: null });
    expect(parse("Brunch So.").due).toEqual(date("2026-10-25"));
  });

  it("parses dates, rolling past ones into next year", () => {
    expect(parse("Klausur 24.10.").due).toEqual(date("2026-10-24"));
    expect(parse("Klausur 3.2.").due).toEqual(date("2027-02-03"));
    expect(parse("Klausur 24.10.2027").due).toEqual(date("2027-10-24"));
    expect(parse("Klausur 24.10.27").due).toEqual(date("2027-10-24"));
    expect(parse("Version 2.10 testen").due).toBeNull();
    expect(parse("Klausur 31.2.")).toMatchObject({ title: "Klausur 31.2.", due: null });
  });

  it("parses times, with or without a day", () => {
    expect(parse("Übung Fr 14:00")).toMatchObject({ title: "Übung", due: at(2026, 10, 23, 14) });
    expect(parse("Sprechstunde morgen um 9:30")).toMatchObject({ title: "Sprechstunde", due: at(2026, 10, 19, 9, 30) });
    expect(parse("Anrufen 14 Uhr")).toMatchObject({ title: "Anrufen", due: at(2026, 10, 18, 14) });
    expect(parse("Anrufen 14uhr").due).toEqual(at(2026, 10, 18, 14));
    // schon vorbei → morgen
    expect(parse("Wecker 7:00").due).toEqual(at(2026, 10, 19, 7));
    expect(parse("Raum 25:00").due).toBeNull();
    expect(parse("3 Uhr").due).toBeNull();
  });

  it("drops a preposition only right before the date", () => {
    expect(parse("Abgabe bis Fr").title).toBe("Abgabe");
    expect(parse("Treffen am Bahnhof morgen").title).toBe("Treffen am Bahnhof");
  });

  it("parses priority", () => {
    expect(parse("Bericht !1")).toMatchObject({ title: "Bericht", priority: "high" });
    expect(parse("!2 Bericht").priority).toBe("medium");
    expect(parse("Bericht !3").priority).toBe("low");
    expect(parse("Bericht !4")).toMatchObject({ title: "Bericht !4", priority: null });
  });

  it("resolves @list by normalized name or unique prefix", () => {
    expect(parse("Blatt 3 @uni-analysis", lists)).toMatchObject({ title: "Blatt 3", listId: "a" });
    expect(parse("Milch @eink", lists)).toMatchObject({ title: "Milch", listId: "e" });
    expect(parse("Zettel @eingang", lists).listId).toBe("in");
    // mehrdeutig oder unbekannt → bleibt im Titel
    expect(parse("Blatt @uni", lists)).toMatchObject({ title: "Blatt @uni", listId: null });
    expect(parse("Mail an @bob", lists)).toMatchObject({ title: "Mail an @bob", listId: null });
    expect(parse("Blatt @uni-analysis")).toMatchObject({ title: "Blatt @uni-analysis", listId: null });
  });

  it("combines everything", () => {
    expect(parse("Übungsblatt 3 bis Fr 14:00 !1 @analysis", lists)).toEqual({
      title: "Übungsblatt 3 @analysis",
      due: at(2026, 10, 23, 14),
      priority: "high",
      listId: null,
      recurrence: null,
    });
    expect(parse("Übungsblatt 3 bis Fr 14:00 !1 @uni-ana", lists).listId).toBe("a");
  });
});

describe("parseDue (Datum wählen…)", () => {
  const due = (input: string) => parseDue(input, { now, tz: "Europe/Berlin" });

  it("accepts a bare day, date or time – where parseNewTask keeps it as title", () => {
    expect(parse("24.12.").due).toBeNull();
    expect(due("24.12.")).toEqual(date("2026-12-24"));
    expect(due("fr")).toEqual(date("2026-10-23"));
    expect(due("morgen 14:00")).toEqual(at(2026, 10, 19, 14));
    expect(due("am Fr um 9 Uhr")).toEqual(at(2026, 10, 23, 9));
  });

  it("returns null without a day or time", () => {
    expect(due("")).toBeNull();
    expect(due("quatsch")).toBeNull();
  });
});

describe("recognizedMarks (P6-01)", () => {
  const marks = (input: string) => recognizedMarks(input, { now, lists, tz: "Europe/Berlin" });
  const texts = (input: string) => marks(input).map((m) => [input.slice(m.start, m.end), m.kind, m.priority ?? null]);

  it("marks the effective hits at their character positions, merging neighbours of one kind", () => {
    expect(texts("Bericht bis Fr 14:00 !1 @einkauf")).toEqual([
      ["bis Fr 14:00", "due", null],
      ["!1", "priority", "high"],
      ["@einkauf", "list", null],
    ]);
    // Positionen beziehen sich auf den unveränderten Text (führende Leerzeichen bleiben).
    expect(marks("  Milch morgen")).toEqual([{ start: 8, end: 14, kind: "due" }]);
  });

  it("marks only the last hit of a kind and nothing when the title would be empty", () => {
    expect(texts("morgen Bericht heute")).toEqual([["heute", "due", null]]);
    expect(texts("Morgen")).toEqual([]);
    expect(texts("Kaufen @unbekannt")).toEqual([]);
  });
});
