// N6-03: Löschen legt in den Papierkorb (ADR-031); Ansicht „Papierkorb“ mit Wiederherstellen,
// endgültigem Löschen (Bestätigung) und „Papierkorb leeren“.
import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function setup() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  for (const t of ["A", "B", "C"]) await client.createTask({ title: t });
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /^A/ });
  return { client, user: userEvent.setup() };
}

const shown = () => within(screen.getByRole("listbox")).queryAllByRole("option").map((r) => r.getAttribute("aria-label"));
const nav = () => screen.getByRole("navigation", { name: "Navigation" });

describe("Papierkorb", () => {
  it("delete moves to the trash; restore brings the task back with the same id", async () => {
    const { client, user } = await setup();
    const [, b] = (await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 })).items;
    await user.click(screen.getByRole("option", { name: "B" }));
    screen.getByRole("option", { name: "B" }).focus();
    await user.keyboard("{Delete}");
    await waitFor(() => expect(shown()).toEqual(["A", "C"]));
    expect(screen.getByText("„B“ in den Papierkorb gelegt")).toBeTruthy();
    // Zähler in der Seitenleiste.
    await waitFor(() => expect(within(nav()).getByRole("link", { name: /^Papierkorb/ }).textContent).toContain("1"));

    await user.click(within(nav()).getByRole("link", { name: /^Papierkorb/ }));
    await screen.findByRole("heading", { name: "Papierkorb", level: 1 });
    const row = await screen.findByRole("option", { name: "B" });
    expect(row.textContent).toContain("heute gelöscht");
    expect(within(row).queryByRole("checkbox")).toBeNull();
    await user.click(within(row).getByRole("button", { name: "Wiederherstellen" }));
    expect(await screen.findByText(/Der Papierkorb ist leer/)).toBeTruthy();
    expect((await client.getTask(b.id)).trashedAt).toBeNull();
  });

  it("purges after confirming and empties the trash", async () => {
    const { client, user } = await setup();
    const ids = (await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 })).items.map((t) => t.id);
    await client.trashTasks(ids);
    await user.click(within(nav()).getByRole("link", { name: /^Papierkorb/ }));
    await screen.findByRole("option", { name: "A" });

    // Entf im Papierkorb fragt nach.
    await user.click(screen.getByRole("option", { name: "A" }));
    screen.getByRole("option", { name: "A" }).focus();
    await user.keyboard("{Delete}");
    const confirm = await screen.findByRole("dialog", { name: "Endgültig löschen?" });
    await user.click(within(confirm).getByRole("button", { name: "Endgültig löschen" }));
    await waitFor(async () => expect(await client.listTasks({ view: { kind: "trash" } }, { limit: 10 })).toMatchObject({ items: [{}, {}] }));

    await waitFor(() => expect(within(nav()).getByRole("link", { name: /^Papierkorb/ }).textContent).toContain("2"));
    await user.click(screen.getByRole("button", { name: "Aktionen für Papierkorb" }));
    await user.click(within(await screen.findByRole("menu", { name: "Aktionen für Papierkorb" })).getByRole("menuitem", { name: /Papierkorb leeren/ }));
    const all = await screen.findByRole("dialog", { name: "Endgültig löschen?" });
    expect(all.textContent).toContain("alle 2 Tasks im Papierkorb");
    await user.click(within(all).getByRole("button", { name: "Endgültig löschen" }));
    await waitFor(async () => expect((await client.listTasks({ view: { kind: "trash" } }, { limit: 10 })).items).toHaveLength(0));
    expect(await screen.findByText("2 Tasks endgültig gelöscht")).toBeTruthy();
  });
});
