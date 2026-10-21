import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { dueToInputDate, dueToInputTime } from "../../lib/format-date";
import { isoDate, addDays } from "../../lib/local-day";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function setup() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const work = await client.createTaskList("Arbeit");
  const task = await client.createTask({ title: "Bericht" });
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /^Bericht/ });
  return { client, work, task, user: userEvent.setup() };
}

describe("Neue Task (Fenster)", () => {
  /** „+ Neue Task“ öffnet das Fenster; liefert sein Eingabefeld (fokussiert). */
  async function openNewTask(user: ReturnType<typeof userEvent.setup>) {
    await user.click(within(screen.getByRole("navigation", { name: "Navigation" })).getByRole("button", { name: /^Neue Task/ }));
    const dialog = await screen.findByRole("dialog", { name: "Neue Task" });
    const input = within(dialog).getByRole("textbox", { name: "Neue Task" });
    await waitFor(() => expect(document.activeElement).toBe(input));
    return input;
  }

  it("creates on Enter, parses the day word and closes", async () => {
    const { client, user } = await setup();
    await openNewTask(user);
    await user.keyboard("Zahnarzt anrufen morgen");
    expect(within(screen.getByRole("dialog")).getByText("Morgen")).toBeTruthy();
    await user.keyboard("{Enter}");
    await screen.findByRole("option", { name: /^Zahnarzt anrufen/ });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const page = await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 });
    const created = page.items.find((t) => t.title === "Zahnarzt anrufen")!;
    expect(created.due).toEqual({ kind: "date", date: isoDate(addDays(new Date(), 1)) });
  });

  it("adds to the open list and defaults to today in the Today view", async () => {
    const { client, work, user } = await setup();
    location.hash = `#/list/${work.id}`;
    await screen.findByRole("heading", { name: "Arbeit" });
    await openNewTask(user);
    await user.keyboard("Review{Enter}");
    await screen.findByRole("option", { name: /^Review/ });
    location.hash = "#/today";
    await screen.findByRole("heading", { name: "Heute" });
    await openNewTask(user);
    await user.keyboard("Anrufen{Enter}");
    await screen.findByRole("option", { name: /^Anrufen/ });
    const inList = await client.listTasks({ view: { kind: "list", listId: work.id } }, { limit: 10 });
    expect(inList.items.map((t) => t.title)).toEqual(["Review"]);
  });

  it("the main pane has no quick-add row any more", async () => {
    await setup();
    expect(screen.queryByRole("textbox", { name: "Neue Task" })).toBeNull();
  });
});

describe("TaskDetail", () => {
  it("opens on click and saves title on blur, notes after a pause", async () => {
    const { client, task, user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Bericht/ }));
    const title = await screen.findByRole("textbox", { name: "Titel" });
    await user.clear(title);
    await user.type(title, "Quartalsbericht");
    await user.click(screen.getByRole("textbox", { name: "Notizen" }));
    await waitFor(async () => expect((await client.getTask(task.id)).title).toBe("Quartalsbericht"));
    await screen.findByRole("option", { name: /^Quartalsbericht/ });
    await user.keyboard("Seite 3 prüfen");
    await waitFor(async () => expect((await client.getTask(task.id)).notes).toBe("Seite 3 prüfen"), { timeout: 2000 });
  });

  it("rejects an empty title and restores it", async () => {
    const { client, task, user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Bericht/ }));
    const title = await screen.findByRole("textbox", { name: "Titel" });
    await user.clear(title);
    expect(await screen.findByRole("alert")).toBeTruthy();
    await user.click(screen.getByRole("textbox", { name: "Notizen" }));
    await waitFor(() => expect((title as HTMLTextAreaElement).value).toBe("Bericht"));
    expect((await client.getTask(task.id)).title).toBe("Bericht");
  });

  it("sets due, priority and moves to another list", async () => {
    const { client, task, work, user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Bericht/ }));
    await user.click(await screen.findByRole("button", { name: "Fällig: kein Datum" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Fälligkeit wählen" })).getByRole("button", { name: "Morgen" }));
    await user.click(screen.getByRole("radio", { name: "Hoch" }));
    const detail = screen.getByRole("region", { name: "Task-Details" });
    await user.click(within(detail).getByRole("button", { name: "Liste: Eingang" }));
    await user.click(within(await screen.findByRole("menu", { name: "Liste wählen" })).getByRole("menuitem", { name: work.name }));
    await waitFor(async () => {
      const t = await client.getTask(task.id);
      expect(t.priority).toBe("high");
      expect(t.due).toEqual({ kind: "date", date: isoDate(addDays(new Date(), 1)) });
      expect(t.listId).toBe(work.id);
    });
    // Aus dem Eingang verschwunden.
    await waitFor(() => expect(screen.queryByRole("option", { name: /^Bericht/ })).toBeNull());
  });

  it("sets a time, keeps it when the day changes and clears it to all-day (P5-08)", async () => {
    const { client, task, user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Bericht/ }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    const field = () => detail.querySelector<HTMLButtonElement>("#detail-due")!;
    const picker = async () => {
      await user.click(field());
      return within(await screen.findByRole("dialog", { name: "Fälligkeit wählen" }));
    };
    const due = async () => (await client.getTask(task.id)).due;
    const today = isoDate(new Date());
    const tomorrow = isoDate(addDays(new Date(), 1));

    // Uhrzeit ohne Datum → heute um diese Uhrzeit; das Popover schließt.
    await user.type((await picker()).getByRole("textbox", { name: "Uhrzeit" }), "14:00{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Fälligkeit wählen" })).toBeNull());
    await waitFor(async () => {
      const d = await due();
      expect(d?.kind).toBe("dateTime");
      expect(dueToInputDate(d)).toBe(today);
      expect(dueToInputTime(d)).toBe("14:00");
    });
    expect(document.activeElement).toBe(field());

    // Schnellwahl und Text behalten die Uhrzeit.
    await user.click((await picker()).getByRole("button", { name: "Morgen" }));
    await waitFor(async () => expect(dueToInputDate(await due())).toBe(tomorrow));
    expect(dueToInputTime(await due())).toBe("14:00");
    await waitFor(() => expect(field().dataset.value).toBe(tomorrow));
    await user.type((await picker()).getByRole("textbox", { name: "Fällig am" }), "15.01.2030{Enter}");
    await waitFor(async () => expect(dueToInputDate(await due())).toBe("2030-01-15"));
    expect(dueToInputTime(await due())).toBe("14:00");

    // Raster per Tastatur: gewählter Tag markiert, → einen Tag, Bild↓ einen Monat, Enter wählt.
    await waitFor(() => expect(field().dataset.value).toBe("2030-01-15"));
    const p = await picker();
    const cell = p.getByRole("gridcell", { name: /15\. Januar 2030/ });
    expect(cell.getAttribute("aria-selected")).toBe("true");
    cell.focus();
    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(document.activeElement?.getAttribute("data-date")).toBe("2030-01-16"));
    await user.keyboard("{PageDown}");
    await waitFor(() => expect(document.activeElement?.getAttribute("data-date")).toBe("2030-02-16"));
    expect(p.getByRole("grid", { name: "Februar 2030" })).toBeTruthy();
    await user.keyboard("{Enter}");
    await waitFor(async () => expect(dueToInputDate(await due())).toBe("2030-02-16"));
    expect(dueToInputTime(await due())).toBe("14:00");

    // Uhrzeit leeren → ganztägig, Datum bleibt; „Entfernen“ löscht die Fälligkeit.
    await waitFor(() => expect(field().dataset.time).toBe("14:00"));
    const time = (await picker()).getByRole("textbox", { name: "Uhrzeit" }) as HTMLInputElement;
    expect(time.value).toBe("14:00");
    await user.clear(time);
    await user.keyboard("{Enter}");
    await waitFor(async () => expect(await due()).toEqual({ kind: "date", date: "2030-02-16" }));
    await waitFor(() => expect(field().dataset.time).toBe(""));
    await user.click((await picker()).getByRole("button", { name: "Entfernen" }));
    await waitFor(async () => expect(await due()).toBeNull());
  });

  it("closes the due popover with Esc and keeps the detail open", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Bericht/ }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await user.click(within(detail).getByRole("button", { name: "Fällig: kein Datum" }));
    await screen.findByRole("dialog", { name: "Fälligkeit wählen" });
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Fälligkeit wählen" })).toBeNull());
    expect(screen.getByRole("region", { name: "Task-Details" })).toBeTruthy();
  });

  it("closes with the close button", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Bericht/ }));
    await user.click(await screen.findByRole("button", { name: "Schließen" }));
    expect(screen.queryByRole("region", { name: "Task-Details" })).toBeNull();
  });
});
