import type { TaskListDto } from "@tasks/core-client";
import { describe, expect, it } from "vitest";
import { parseDue, parseNewTask } from "./parse";

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

describe("parseNewTask", () => {
  it("extracts simple day words anywhere", () => {
    expect(parse("Zahnarzt anrufen morgen")).toEqual({
      title: "Zahnarzt anrufen",
      due: date("2026-10-19"),
      priority: null,
      listId: null,
    });
    expect(parse("Heute Milch kaufen").due).toEqual(date("2026-10-18"));
    expect(parse("Bericht übermorgen fertig")).toMatchObject({ title: "Bericht fertig", due: date("2026-10-20") });
  });

  it("ignores partial words and keeps lone words as title", () => {
    expect(parse("Morgenroutine planen").due).toBeNull();
    expect(parse("Heutemorgen").due).toBeNull();
    expect(parse("morgen")).toEqual({ title: "morgen", due: null, priority: null, listId: null });
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
