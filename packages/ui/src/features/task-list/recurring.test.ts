// N4-06: Serien in Liste, Detail und Sammelaktionen – Icon, Abhaken mit „nächstes Mal“ und Rückgängig,
// Überspringen, Beenden, gemischtes Sammel-Abhaken.
import { FakeCoreClient, type RecurrenceDto } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { dueLabel, todayPlus } from "../../lib/format-date";
import { completeManyWithUndo, nextTimeLabel } from "../../lib/task-actions";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

const daily: RecurrenceDto = { freq: "daily", interval: 1, weekdays: [], anchor: "schedule" };

async function setup() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  const series = await client.createTask({ title: "Gießen", due: todayPlus(0), recurrence: daily });
  const plain = await client.createTask({ title: "Einmalig" });
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /^Gießen/ });
  return { client, series, plain, user: userEvent.setup() };
}

describe("Serien (N4-06)", () => {
  it("row shows the repeat icon; completing keeps the row, advances and undoes", async () => {
    const { client, series, user } = await setup();
    const row = screen.getByRole("option", { name: /^Gießen/ });
    expect(row.getAttribute("aria-label")).toContain("wiederholt sich");
    expect(within(row).getByLabelText("wiederholt sich").getAttribute("title")).toBe("Wiederholt sich");

    await user.click(within(row).getByRole("checkbox", { name: "Erledigt: Gießen" }));
    const message = `„Gießen“ erledigt – nächstes Mal ${nextTimeLabel(todayPlus(1))}`;
    expect(await screen.findByText(message)).toBeTruthy();
    expect((await client.getTask(series.id)).due).toEqual(todayPlus(1));
    // Zeile bleibt offen stehen (mit neuer Fälligkeit).
    await waitFor(() => expect(screen.getByRole("option", { name: /^Gießen/ }).getAttribute("aria-label")).toContain(dueLabel(todayPlus(1)).long));

    await user.click(screen.getByRole("button", { name: "Rückgängig" }));
    await waitFor(async () => expect((await client.getTask(series.id)).due).toEqual(todayPlus(0)));
    const all = await client.listTasks({ view: { kind: "all" } }, { limit: 10 });
    expect(all.items.map((t) => t.title)).toEqual(["Gießen", "Einmalig"]);
  });

  it("skip from the context menu; end and skip in the detail panel", async () => {
    const { client, series, user } = await setup();
    await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("option", { name: /^Gießen/ }) });
    await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Dieses Mal überspringen" }));
    expect(await screen.findByText(`„Gießen“ übersprungen – nächstes Mal ${nextTimeLabel(todayPlus(1))}`)).toBeTruthy();
    expect((await client.getTask(series.id)).due).toEqual(todayPlus(1));

    await user.click(screen.getByRole("option", { name: /^Gießen/ }));
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await user.click(within(detail).getByRole("button", { name: "Dieses Mal überspringen" }));
    await waitFor(async () => expect((await client.getTask(series.id)).due).toEqual(todayPlus(2)));

    const field = within(detail).getByRole("button", { name: "Wiederholung" });
    expect(field.textContent).toContain("täglich");
    await user.click(field);
    await user.click(within(await screen.findByRole("menu", { name: "Wiederholung wählen" })).getByRole("menuitem", { name: "Wiederholung beenden" }));
    await waitFor(async () => expect((await client.getTask(series.id)).recurrence).toBeNull());
    expect(within(detail).queryByRole("button", { name: "Dieses Mal überspringen" })).toBeNull();
    expect(field.textContent).toContain("Keine");
    await user.click(field);
    await user.click(within(await screen.findByRole("menu", { name: "Wiederholung wählen" })).getByRole("menuitem", { name: "Wöchentlich" }));
    await waitFor(async () => expect((await client.getTask(series.id)).recurrence).toMatchObject({ freq: "weekly" }));
  });

  it("mixed bulk completion: one toast, undo restores both", async () => {
    const { client, series, plain } = await setup();
    const changed = await completeManyWithUndo(client, [series.id, plain.id], true);
    expect(changed?.map((t) => t.title)).toEqual(["Gießen", "Einmalig"]);
    expect(toasts.items.map((t) => t.message)).toEqual(["2 Tasks erledigt"]);
    expect((await client.getTask(plain.id)).status).toBe("done");
    toasts.items[0].onAction!();
    await vi.waitFor(async () => expect((await client.getTask(plain.id)).status).toBe("open"));
    await vi.waitFor(async () => expect((await client.getTask(series.id)).due).toEqual(todayPlus(0)));
    const all = await client.listTasks({ view: { kind: "all" } }, { limit: 10 });
    expect(all.items).toHaveLength(2);
  });
});
