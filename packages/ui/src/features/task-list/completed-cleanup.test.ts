// P4-06: „Erledigt“ nach Tagen und „Erledigte älter als 30 Tage löschen“ mit Rückgängig.
import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { addDays } from "../../lib/local-day";

beforeEach(() => {
  location.hash = "#/completed";
  localStorage.clear();
  toasts.items = [];
});

/** Legt Tasks an und hakt sie „vor n Tagen“ ab (Uhr des Fakes verstellt). */
async function setup(ages: [string, number][]) {
  let clock = new Date();
  const client = new FakeCoreClient({ now: () => clock });
  await client.ensureInbox();
  for (const [title, days] of ages) {
    const t = await client.createTask({ title });
    clock = addDays(new Date(), -days);
    clock.setHours(10);
    await client.completeTask(t.id, true);
    clock = new Date();
  }
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Erledigt" });
  await screen.findByRole("option", { name: new RegExp(`^${ages[0][0]}`) });
  return { client, user: userEvent.setup() };
}

const headings = () => [...document.querySelectorAll(".heading .label")].map((e) => e.textContent);
const titles = () => screen.queryAllByRole("option").map((o) => o.querySelector(".title")?.textContent);

describe("Erledigt", () => {
  it("groups by the day of completion", async () => {
    await setup([
      ["Heute erledigt", 0],
      ["Gestern erledigt", 1],
    ]);
    await waitFor(() => expect(headings()).toEqual(["Heute", "Gestern"]));
  });

  it("deletes completed tasks older than 30 days after confirming, with undo", async () => {
    const { client, user } = await setup([
      ["Neu", 2],
      ["Alt", 40],
      ["Uralt", 400],
    ]);
    await user.click(screen.getByRole("button", { name: "Aktionen für Erledigt" }));
    await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: /älter als 30 Tage/ }));
    const dialog = await screen.findByRole("dialog", { name: "Erledigte aufräumen?" });
    expect(dialog.textContent).toContain("2 erledigte Tasks");
    await user.click(within(dialog).getByRole("button", { name: "2 Tasks löschen" }));
    await waitFor(() => expect(titles()).toEqual(["Neu"]));
    const toast = await screen.findByRole("status");
    await user.click(within(toast).getByRole("button", { name: "Rückgängig" }));
    await waitFor(() => expect(titles()).toEqual(["Neu", "Alt", "Uralt"]));
    const all = await client.listTasks({ view: { kind: "completed" } }, { limit: 10 });
    expect(all.items).toHaveLength(3);
  });

  it("says so when nothing is old enough", async () => {
    const { user } = await setup([["Neu", 3]]);
    await user.click(screen.getByRole("button", { name: "Aktionen für Erledigt" }));
    await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: /älter als 30 Tage/ }));
    expect((await screen.findByRole("status")).textContent).toContain("Keine erledigten Tasks älter als 30 Tage");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
