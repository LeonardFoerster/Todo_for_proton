// P4-04: Fälligkeit per Kürzel (T, Umschalt+T, D) und für die Auswahl.
import { FakeCoreClient, type DueDto } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { dueToInputTime } from "../../lib/format-date";
import { addDays, isoDate } from "../../lib/local-day";
import { dueChoices, dueResult } from "./due-actions";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

const date = (offset: number): DueDto => ({ kind: "date", date: isoDate(addDays(new Date(), offset)) });

async function setup(tasks: [string, DueDto | null][] = [["Task 1", null], ["Task 2", null]]) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const ids: string[] = [];
  for (const [title, due] of tasks) ids.push((await client.createTask({ title, due })).id);
  render(App, { props: { client } });
  await screen.findByRole("option", { name: new RegExp(`^${tasks[0][0]}`) });
  return { client, ids, user: userEvent.setup() };
}

const dueOf = async (client: FakeCoreClient, id: string) => (await client.getTask(id)).due;

describe("dueChoices / dueResult", () => {
  it("offers next Monday, never today", () => {
    const monday = new Date(2026, 9, 19, 10);
    expect(dueChoices(monday).find((c) => c.id === "monday")?.due).toEqual({ kind: "date", date: "2026-10-26" });
    const sunday = new Date(2026, 9, 18, 10);
    expect(dueChoices(sunday).find((c) => c.id === "monday")?.due).toEqual({ kind: "date", date: "2026-10-19" });
  });

  it("names the result", () => {
    const now = new Date(2026, 9, 19, 10);
    expect(dueResult({ kind: "date", date: "2026-10-19" }, now)).toBe("auf heute verschoben");
    expect(dueResult({ kind: "date", date: "2026-10-23" }, now)).toBe("auf Fr, 23. Okt. verschoben");
    expect(dueResult({ kind: "date", date: "2027-02-03" }, now)).toBe("auf 3. Feb. 2027 verschoben");
    expect(dueResult(null, now)).toBe("Fälligkeit entfernt");
  });
});

describe("Fälligkeit per Tastatur", () => {
  it("T sets today, Umschalt+T tomorrow – keeping the time – with undo", async () => {
    const at = addDays(new Date(), 5);
    at.setHours(14, 30, 0, 0);
    const { client, ids, user } = await setup([["Termin", { kind: "dateTime", at: at.toISOString(), tz: "Europe/Berlin" }]]);
    await user.keyboard("{ArrowDown}t");
    await waitFor(async () => expect(isoDate(new Date(((await dueOf(client, ids[0])) as { at: string }).at))).toBe(isoDate(new Date())));
    expect(dueToInputTime(await dueOf(client, ids[0]))).toBe("14:30");
    expect((await screen.findByRole("status")).textContent).toContain("„Termin“ auf heute verschoben");

    await user.keyboard("{Shift>}T{/Shift}");
    await waitFor(async () => expect(isoDate(new Date(((await dueOf(client, ids[0])) as { at: string }).at))).toBe(isoDate(addDays(new Date(), 1))));
    const toast = screen.getAllByRole("status").at(-1)!;
    await user.click(within(toast).getByRole("button", { name: "Rückgängig" }));
    await waitFor(async () => expect(isoDate(new Date(((await dueOf(client, ids[0])) as { at: string }).at))).toBe(isoDate(new Date())));
  });

  it("D opens the menu; „Datum wählen…“ parses text like the title line", async () => {
    const { client, ids, user } = await setup();
    await user.keyboard("{ArrowDown}d");
    const menu = await screen.findByRole("menu", { name: "Fälligkeit" });
    await user.click(within(menu).getByRole("menuitem", { name: /Nächster Montag/ }));
    await waitFor(async () => expect(await dueOf(client, ids[0])).toEqual(dueChoices().find((c) => c.id === "monday")!.due));

    await user.keyboard("d");
    await user.click(within(await screen.findByRole("menu", { name: "Fälligkeit" })).getByRole("menuitem", { name: /Datum wählen/ }));
    const dialog = await screen.findByRole("dialog", { name: /Fälligkeit für „Task 1“/ });
    const field = within(dialog).getByRole("textbox", { name: "Fällig am" });
    await waitFor(() => expect(document.activeElement).toBe(field));
    await user.keyboard("quatsch{Enter}");
    expect(within(dialog).getByRole("alert").textContent).toContain("Kein Datum erkannt");
    await user.clear(field);
    await user.keyboard("24.12.");
    expect(within(dialog).getByText(/24\. Dezember/)).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const due = await dueOf(client, ids[0]);
    expect(due?.kind === "date" && due.date.endsWith("-12-24")).toBe(true);

    // Entfernen nur, wenn eine Fälligkeit gesetzt ist.
    await user.keyboard("d");
    await user.click(within(await screen.findByRole("menu", { name: "Fälligkeit" })).getByRole("menuitem", { name: /Fälligkeit entfernen/ }));
    await waitFor(async () => expect(await dueOf(client, ids[0])).toBeNull());
  });

  it("sets the due of the whole selection in one call", async () => {
    const { client, ids, user } = await setup([["A", null], ["B", date(3)], ["C", null]]);
    let calls = 0;
    const original = client.setDues;
    client.setDues = (d) => {
      calls++;
      return original(d);
    };
    await user.keyboard("{Control>}a{/Control}");
    await waitFor(() => expect(screen.getByRole("listbox").getAttribute("aria-multiselectable")).toBe("true"));
    await user.click(screen.getByRole("button", { name: "Fälligkeit…" }));
    await user.click(within(await screen.findByRole("menu", { name: "Fälligkeit" })).getByRole("menuitem", { name: /^Morgen/ }));
    await waitFor(async () => {
      for (const id of ids) expect(await dueOf(client, id)).toEqual(date(1));
    });
    expect(calls).toBe(1);
    expect(screen.getAllByRole("status").at(-1)!.textContent).toContain("3 Tasks auf morgen verschoben");
  });

  it("does not fire while typing", async () => {
    const { client, ids, user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Task 1/ }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await user.click(within(detail).getByRole("textbox", { name: "Notizen" }));
    await user.keyboard("td");
    expect(await dueOf(client, ids[0])).toBeNull();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("offers „Fälligkeit…“ in the row context menu", async () => {
    const { client, ids, user } = await setup();
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("option", { name: /^Task 2/ }) });
    await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: /Fälligkeit…/ }));
    await user.click(within(await screen.findByRole("menu", { name: "Fälligkeit" })).getByRole("menuitem", { name: /^Heute/ }));
    await waitFor(async () => expect(await dueOf(client, ids[1])).toEqual(date(0)));
  });

  it("works on the board cursor card", async () => {
    const { client, ids, user } = await setup();
    await user.keyboard("v");
    await screen.findByRole("region", { name: "Kanban-Board" });
    await user.keyboard("{ArrowDown}{ArrowDown}");
    await user.keyboard("{Shift>}T{/Shift}");
    await waitFor(async () => expect(await dueOf(client, ids[1])).toEqual(date(1)));
    expect(await dueOf(client, ids[0])).toBeNull();
  });
});
