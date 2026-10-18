import { FakeCoreClient } from "@tasks/core-client";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  localStorage.setItem("tasks.layout", "board");
  toasts.items = [];
});

async function setup() {
  const client = new FakeCoreClient();
  const inbox = await client.ensureInbox();
  const shop = await client.createTaskList("Einkaufen");
  const work = await client.createTaskList("Arbeit");
  const ids: Record<string, string> = {};
  for (const [title, listId] of [
    ["E1", undefined],
    ["E2", undefined],
    ["Milch", shop.id],
    ["Brot", shop.id],
    ["Käse", shop.id],
    ["Bericht", work.id],
  ] as const) {
    ids[title] = (await client.createTask({ title, listId })).id;
  }
  await client.completeTask((await client.createTask({ title: "Alt", listId: shop.id })).id, true);
  render(App, { props: { client } });
  await screen.findByRole("region", { name: "Kanban-Board" });
  await screen.findByRole("option", { name: "Milch" });
  return { client, inbox, shop, work, ids, user: userEvent.setup() };
}

const board = () => screen.getByRole("region", { name: "Kanban-Board" });
const columns = () => within(board()).getAllByRole("region").map((c) => c.getAttribute("aria-label"));
const column = (name: string) => within(board()).getByRole("region", { name });
const cards = (name: string) =>
  within(column(name))
    .queryAllByRole("option")
    .map((o) => o.getAttribute("aria-label"));
const cursor = () =>
  within(board())
    .queryAllByRole("option")
    .find((o) => o.getAttribute("aria-selected") === "true")
    ?.getAttribute("aria-label");

describe("kanban board", () => {
  it("shows all lists as columns (inbox first) with their open tasks in order", async () => {
    await setup();
    expect(columns()).toEqual(["Eingang", "Einkaufen", "Arbeit"]);
    expect(cards("Eingang")).toEqual(["E1", "E2"]);
    expect(cards("Einkaufen"), "erledigte fehlen").toEqual(["Milch", "Brot", "Käse"]);
    expect(within(column("Einkaufen")).getByLabelText("3 offene Tasks")).toBeTruthy();
    expect(within(column("Eingang")).queryByRole("button", { name: /Listenaktionen/ }), "Eingang ohne Menü").toBeNull();
  });

  it("highlights the column of the chosen list", async () => {
    const { shop } = await setup();
    location.hash = `#/list/${shop.id}`;
    await screen.findByRole("heading", { name: "Einkaufen" });
    await waitFor(() => expect(column("Einkaufen").classList.contains("highlighted")).toBe(true));
    expect(column("Eingang").classList.contains("highlighted")).toBe(false);
  });

  it("follows changes from elsewhere: moves, new, renamed and deleted lists", async () => {
    const { client, work, ids } = await setup();
    await client.moveTask(ids.Milch, { listId: work.id });
    await waitFor(() => expect(cards("Arbeit")).toEqual(["Bericht", "Milch"]));
    expect(cards("Einkaufen")).toEqual(["Brot", "Käse"]);
    const garden = await client.createTaskList("Garten");
    await waitFor(() => expect(columns()).toEqual(["Eingang", "Einkaufen", "Arbeit", "Garten"]));
    await client.renameTaskList(garden.id, "Balkon");
    await waitFor(() => expect(columns()).toContain("Balkon"));
    await client.deleteTaskList(work.id);
    await waitFor(() => expect(columns()).toEqual(["Eingang", "Einkaufen", "Balkon"]));
    await waitFor(() => expect(cards("Eingang")).toEqual(expect.arrayContaining(["Bericht", "Milch"])));
  });

  it("completing a card lets it leave its column; undo brings it back", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("checkbox", { name: "Erledigt: Brot" }));
    await waitFor(() => expect(cards("Einkaufen")).toEqual(["Milch", "Käse"]));
    await user.click(await screen.findByRole("button", { name: "Rückgängig" }));
    await waitFor(() => expect(cards("Einkaufen")).toEqual(["Milch", "Brot", "Käse"]));
  });

  it("navigates with arrows between cards and columns; Enter, Space and Entf act on the cursor", async () => {
    const { client, shop, user } = await setup();
    location.hash = `#/list/${shop.id}`;
    await screen.findByRole("heading", { name: "Einkaufen" });
    await user.keyboard("{ArrowDown}");
    expect(cursor(), "erste Karte der hervorgehobenen Spalte").toBe("Milch");
    await user.keyboard("{ArrowDown}");
    expect(cursor()).toBe("Brot");
    await user.keyboard("{ArrowDown}");
    expect(cursor()).toBe("Käse");
    await user.keyboard("{ArrowLeft}");
    expect(cursor(), "gleiche Höhe, begrenzt").toBe("E2");
    await user.keyboard("l");
    expect(cursor(), "zurück auf gleicher Höhe (2. Karte)").toBe("Brot");
    expect(document.activeElement?.getAttribute("aria-label")).toBe("Brot");
    await user.keyboard("j");

    await user.keyboard("{Enter}");
    expect(await screen.findByRole("region", { name: "Task-Details" })).toBeTruthy();
    await user.keyboard("{Escape}");

    await user.keyboard(" ");
    await waitFor(() => expect(cards("Einkaufen")).toEqual(["Milch", "Brot"]));
    expect(cursor(), "Cursor springt weiter").toBe("Brot");
    await user.keyboard("{Delete}");
    await waitFor(() => expect(cards("Einkaufen")).toEqual(["Milch"]));
    const open = await client.listTasks({ view: { kind: "list", listId: shop.id }, status: "open" }, { limit: 10 });
    expect(open.items.map((t) => t.title)).toEqual(["Milch"]);
  });

  it("opens the detail panel on click", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("option", { name: "Bericht" }));
    expect(await screen.findByRole("region", { name: "Task-Details" })).toBeTruthy();
    expect(cursor()).toBe("Bericht");
  });

  it("virtualizes long columns", async () => {
    const { client, work } = await setup();
    for (let i = 0; i < 300; i++) await client.createTask({ title: `Viel ${i}`, listId: work.id });
    await waitFor(() => expect(within(column("Arbeit")).getByLabelText(/offene Tasks/).textContent).toBe("301"));
    expect(within(column("Arbeit")).getAllByRole("option").length).toBeLessThan(100);
  });
});

describe("moving cards", () => {
  /** jsdom hat kein Layout: Spalten je 280 px breit nebeneinander, Karten 64 px ab y = 44. */
  function layout() {
    const cols = [...document.querySelectorAll<HTMLElement>('[aria-label="Kanban-Board"] > section')];
    cols.forEach((section, i) => {
      const left = i * 292;
      section.getBoundingClientRect = () =>
        ({ left, right: left + 280, top: 0, bottom: 700, width: 280, height: 700 }) as DOMRect;
      const viewport = section.querySelector<HTMLElement>('[role="listbox"]')!;
      viewport.getBoundingClientRect = () =>
        ({ left, right: left + 280, top: 44, bottom: 700, width: 280, height: 656 }) as DOMRect;
    });
    board().getBoundingClientRect = () => ({ left: 0, right: 2000, top: 0, bottom: 700 }) as DOMRect;
  }
  const at = (column: number, slot: number) => ({ clientX: column * 292 + 100, clientY: 44 + slot * 64 });

  async function dragCard(name: string, to: { clientX: number; clientY: number }) {
    const card = screen.getByRole("option", { name });
    const r = { clientX: 10, clientY: 10 };
    await fireEvent.pointerDown(card, { button: 0, ...r });
    await fireEvent.pointerMove(window, { clientX: r.clientX + 10, clientY: r.clientY + 10 });
    await fireEvent.pointerMove(window, to);
    await new Promise((res) => requestAnimationFrame(res));
    await fireEvent.pointerUp(window, to);
  }

  it("drags a card into another column at the drop position – the task really changes its list", async () => {
    const { client, work, ids, user } = await setup();
    layout();
    const move = vi.spyOn(client, "moveTasks");
    const single = vi.spyOn(client, "moveTask");
    await dragCard("Brot", at(2, 1)); // Spalte „Arbeit“, hinter „Bericht“
    await waitFor(() => expect(cards("Arbeit")).toEqual(["Bericht", "Brot"]));
    expect(cards("Einkaufen")).toEqual(["Milch", "Käse"]);
    expect(move, "ein Core-Aufruf").toHaveBeenCalledTimes(1);
    expect(single).not.toHaveBeenCalled();
    expect((await client.getTask(ids.Brot)).listId).toBe(work.id);
    expect(screen.getByText("„Brot“ nach „Arbeit“ verschoben", { selector: "[role=status].sr-only, .sr-only" })).toBeTruthy();

    await user.click(await screen.findByRole("button", { name: "Rückgängig" }));
    await waitFor(() => expect(cards("Einkaufen")).toEqual(["Milch", "Brot", "Käse"]));
    expect(cards("Arbeit")).toEqual(["Bericht"]);
  });

  it("reorders within a column; dropping in place does nothing", async () => {
    const { client, shop } = await setup();
    layout();
    const move = vi.spyOn(client, "moveTask");
    await dragCard("Milch", at(1, 3)); // ans Ende
    await waitFor(() => expect(cards("Einkaufen")).toEqual(["Brot", "Käse", "Milch"]));
    const open = await client.listTasks({ view: { kind: "list", listId: shop.id }, status: "open" }, { limit: 10 });
    expect(open.items.map((t) => t.title)).toEqual(["Brot", "Käse", "Milch"]);
    await dragCard("Brot", at(1, 0));
    expect(move).toHaveBeenCalledTimes(1);
  });

  it("Esc cancels a drag; a plain click still opens the detail", async () => {
    const { client } = await setup();
    layout();
    const move = vi.spyOn(client, "moveTask");
    const card = screen.getByRole("option", { name: "Milch" });
    await fireEvent.pointerDown(card, { button: 0, clientX: 10, clientY: 10 });
    await fireEvent.pointerMove(window, at(2, 0));
    await fireEvent.keyDown(window, { key: "Escape" });
    await fireEvent.pointerUp(window, at(2, 0));
    expect(move).not.toHaveBeenCalled();
    expect(cards("Einkaufen")).toEqual(["Milch", "Brot", "Käse"]);
    await userEvent.setup().click(screen.getByRole("option", { name: "Käse" }));
    expect(await screen.findByRole("region", { name: "Task-Details" })).toBeTruthy();
  });

  it("puts the card back when saving fails", async () => {
    const { client } = await setup();
    layout();
    vi.spyOn(client, "moveTask").mockRejectedValueOnce(new Error("kaputt"));
    vi.spyOn(client, "moveTasks").mockRejectedValueOnce(new Error("kaputt"));
    await dragCard("Milch", at(2, 0));
    await screen.findByText(/Nicht verschoben: kaputt/);
    await waitFor(() => expect(cards("Einkaufen")).toEqual(["Milch", "Brot", "Käse"]));
    expect(cards("Arbeit")).toEqual(["Bericht"]);
  });

  it("Alt+arrows move the cursor card between and within columns", async () => {
    const { client, shop, work, ids, user } = await setup();
    location.hash = `#/list/${shop.id}`;
    await screen.findByRole("heading", { name: "Einkaufen" });
    await user.keyboard("{ArrowDown}{ArrowDown}"); // Brot
    await user.keyboard("{Alt>}{ArrowRight}{/Alt}");
    await waitFor(() => expect(cards("Arbeit")).toEqual(["Bericht", "Brot"]));
    expect(cursor()).toBe("Brot");
    expect((await client.getTask(ids.Brot)).listId).toBe(work.id);
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    await waitFor(() => expect(cards("Arbeit")).toEqual(["Brot", "Bericht"]));
    await user.keyboard("{Alt>}{ArrowLeft}{ArrowLeft}{/Alt}");
    await waitFor(() => expect(cards("Eingang")).toEqual(["Brot", "E1", "E2"]));
  });
});

describe("reordering columns", () => {
  function layoutColumns() {
    const cols = [...document.querySelectorAll<HTMLElement>('[aria-label="Kanban-Board"] > section')];
    cols.forEach((section, i) => {
      const left = i * 292;
      section.getBoundingClientRect = () => ({ left, right: left + 280, width: 280, top: 0, bottom: 700 }) as DOMRect;
    });
    board().getBoundingClientRect = () => ({ left: 0, right: 2000, top: 0, bottom: 700 }) as DOMRect;
  }
  const sidebar = () =>
    [...screen.getByRole("navigation", { name: "Navigation" }).querySelectorAll("a[data-list-id]")].map((a) =>
      a.textContent?.trim(),
    );

  it("drags a column header; board and sidebar share the new order, the inbox stays first", async () => {
    const { client } = await setup();
    await client.createTaskList("Garten");
    await waitFor(() => expect(columns()).toEqual(["Eingang", "Einkaufen", "Arbeit", "Garten"]));
    layoutColumns();
    const header = column("Garten").querySelector("header")!;
    await fireEvent.pointerDown(header, { button: 0, clientX: 900 });
    await fireEvent.pointerMove(window, { clientX: 300 }); // vor „Einkaufen“ (Mitte 432)
    expect(board().querySelector(".column-drop")).toBeTruthy();
    await fireEvent.pointerUp(window, { clientX: 300 });
    await waitFor(() => expect(columns()).toEqual(["Eingang", "Garten", "Einkaufen", "Arbeit"]));
    expect(sidebar()).toEqual(["Garten", "Einkaufen", "Arbeit"]);
    expect((await client.listTaskLists()).map((l) => l.name)).toEqual(["Inbox", "Garten", "Einkaufen", "Arbeit"]);
    expect(column("Eingang").querySelector("[data-column-header]"), "Eingang nicht verschiebbar").toBeNull();
  });

  it("Alt+Left/Right on a focused column header", async () => {
    const { client, user } = await setup();
    const header = column("Einkaufen").querySelector<HTMLElement>("[data-column-header]")!;
    header.focus();
    await user.keyboard("{Alt>}{ArrowRight}{/Alt}");
    await waitFor(() => expect(columns()).toEqual(["Eingang", "Arbeit", "Einkaufen"]));
    expect(document.activeElement?.getAttribute("data-column-header")).toBe(header.dataset.columnHeader);
    await user.keyboard("{Alt>}{ArrowRight}{/Alt}");
    expect(columns(), "schon am Ende").toEqual(["Eingang", "Arbeit", "Einkaufen"]);
    await user.keyboard("{Alt>}{ArrowLeft}{/Alt}");
    await waitFor(() => expect(columns()).toEqual(["Eingang", "Einkaufen", "Arbeit"]));
    expect((await client.listTaskLists()).map((l) => l.name)).toEqual(["Inbox", "Einkaufen", "Arbeit"]);
  });

  it("puts the column back when saving fails", async () => {
    const { client, user } = await setup();
    vi.spyOn(client, "moveTaskList").mockRejectedValueOnce(new Error("kaputt"));
    column("Arbeit").querySelector<HTMLElement>("[data-column-header]")!.focus();
    await user.keyboard("{Alt>}{ArrowLeft}{/Alt}");
    await screen.findByText(/Liste nicht verschoben: kaputt/);
    await waitFor(() => expect(columns()).toEqual(["Eingang", "Einkaufen", "Arbeit"]));
  });
});
