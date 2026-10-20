// P4-02/P4-03: Abschnitte in „Heute“ (Überfällig/Heute) und „Geplant“ (nach Tagen).
import { FakeCoreClient, type DueDto } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { addDays, isoDate } from "../../lib/local-day";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

const day = (offset: number): DueDto => ({ kind: "date", date: isoDate(addDays(new Date(), offset)) });

/** Legt Tasks mit Fälligkeit an und öffnet `hash`. */
async function setup(tasks: [string, DueDto][], hash: string) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  for (const [title, due] of tasks) await client.createTask({ title, due });
  location.hash = hash;
  render(App, { props: { client } });
  await screen.findByRole("option", { name: new RegExp(`^${tasks[0][0]}`) });
  return { client, user: userEvent.setup() };
}

/** Einträge der Liste in Anzeigereihenfolge: Überschriften als „# …“, Tasks mit Titel. */
function shown(): string[] {
  const box = screen.getByRole("listbox");
  return [...box.querySelectorAll<HTMLElement>('[role="option"], [role="presentation"]')].map((el) =>
    el.getAttribute("role") === "option" ? el.querySelector(".title")!.textContent! : `# ${el.querySelector(".label")!.textContent} ${el.querySelector(".count")!.textContent}`,
  );
}

const selected = () => screen.queryAllByRole("option").find((o) => o.getAttribute("aria-selected") === "true");

describe("Heute: Überfällig + Heute", () => {
  const tasks: [string, DueDto][] = [
    ["Heute fällig", day(0)],
    ["Gestern", day(-1)],
    ["Vorige Woche", day(-7)],
  ];

  it("shows the overdue section first, oldest first, with counts", async () => {
    await setup(tasks, "#/today");
    await waitFor(() => expect(shown()).toEqual(["# Überfällig 2", "Vorige Woche", "Gestern", "# Heute 1", "Heute fällig"]));
    // Zeile unter „Heute“ wiederholt den Tag nicht.
    const todayRow = screen.getByRole("option", { name: /^Heute fällig/ });
    expect(todayRow.querySelector(".due")).toBeNull();
    // Seitenleiste: 3 in „Heute“, davon 2 überfällig.
    const nav = screen.getByRole("navigation", { name: "Navigation" });
    await waitFor(() => expect(within(nav).getByRole("link", { name: /^Heute/ }).textContent).toMatch(/3, davon 2 überfällig/));
  });

  it("moves all overdue to today in one call, with undo", async () => {
    const { client, user } = await setup(tasks, "#/today");
    await screen.findByText("Überfällig");
    let calls = 0;
    const original = client.setDues;
    client.setDues = (d) => {
      calls++;
      return original(d);
    };
    await user.click(screen.getByRole("button", { name: "Alle auf heute" }));
    await waitFor(() => expect(shown()).toEqual(["Heute fällig", "Gestern", "Vorige Woche"]));
    expect(calls).toBe(1);
    const toast = await screen.findByRole("status");
    expect(toast.textContent).toContain("2 Tasks auf heute verschoben");
    await user.click(within(toast).getByRole("button", { name: "Rückgängig" }));
    await waitFor(() => expect(shown()[0]).toBe("# Überfällig 2"));
    const page = await client.listTasks({ view: { kind: "all" } }, { limit: 10 });
    expect(page.items.find((t) => t.title === "Vorige Woche")?.due).toEqual(day(-7));
  });

  it("arrow keys skip headings", async () => {
    const { user } = await setup(tasks, "#/today");
    await screen.findByText("Überfällig");
    await user.keyboard("{ArrowDown}");
    expect(selected()?.getAttribute("aria-label")).toMatch(/^Vorige Woche/);
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(selected()?.getAttribute("aria-label")).toMatch(/^Heute fällig/);
    expect(document.activeElement).toBe(selected());
    await user.keyboard("{ArrowUp}");
    expect(selected()?.getAttribute("aria-label")).toMatch(/^Gestern/);
  });

  it("completing the last overdue task removes its section", async () => {
    const { user } = await setup([["Heute fällig", day(0)], ["Gestern", day(-1)]], "#/today");
    await screen.findByText("Überfällig");
    await user.click(screen.getByRole("checkbox", { name: "Erledigt: Gestern" }));
    await waitFor(() => expect(shown()).toEqual(["Heute fällig"]));
  });
});

describe("Geplant: nach Tagen", () => {
  it("groups by day, shows only the time under a day and skips headings with the keyboard", async () => {
    const at = (offset: number, h: number): DueDto => {
      const d = addDays(new Date(), offset);
      d.setHours(h, 0, 0, 0);
      return { kind: "dateTime", at: d.toISOString(), tz: "Europe/Berlin" };
    };
    const { user } = await setup(
      [
        ["Übermorgen", day(2)],
        ["Morgen 9", at(1, 9)],
        ["Morgen", day(1)],
      ],
      "#/upcoming",
    );
    await waitFor(() => expect(shown()[0]).toBe("# Morgen 2"));
    const third = shown()[3];
    expect(shown()).toEqual(["# Morgen 2", "Morgen", "Morgen 9", third, "Übermorgen"]);
    expect(third).toMatch(/^# \w{2}, \d+\. \S+ 1$/);
    expect(screen.getByRole("option", { name: /^Morgen 9/ }).querySelector(".due")?.textContent).toBe("09:00");
    expect(screen.getByRole("option", { name: /^Morgen,/ }).querySelector(".due")).toBeNull();
    await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
    expect(selected()?.getAttribute("aria-label")).toMatch(/^Übermorgen/);
  });
});
