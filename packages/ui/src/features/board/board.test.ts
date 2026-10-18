import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
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
