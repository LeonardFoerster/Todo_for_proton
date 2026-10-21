// N10-03: Gleichnamige Listen zusammenführen (ADR-020) – Hinweis nach dem Laden, Menü „Zusammenführen mit…“.
import { FakeCoreClient } from "@tasks/core-client";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

describe("Listen zusammenführen (N10-03)", () => {
  it("offers to merge same-named lists once and merges them", async () => {
    const client = new FakeCoreClient();
    await client.ensureInbox();
    const a = await client.createTaskList("Arbeit");
    const b = await client.createTaskList("arbeit");
    await client.createTask({ title: "Bericht", listId: b.id });
    render(App, { props: { client } });
    const user = userEvent.setup();
    expect(await screen.findByText("2 Listen heißen „Arbeit“")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Zusammenführen" }));
    await waitFor(async () => expect((await client.listTaskLists()).map((l) => l.name)).toEqual(["Inbox", "Arbeit"]));
    const moved = await client.listTasks({ view: { kind: "list", listId: a.id } }, { limit: 10 });
    expect(moved.items.map((t) => t.title)).toEqual(["Bericht"]);
    expect(await screen.findByText("„arbeit“ in „Arbeit“ zusammengeführt · 1 Task verschoben")).toBeTruthy();
  });

  it("merges via the list menu into another list", async () => {
    const client = new FakeCoreClient();
    await client.ensureInbox();
    await client.createTaskList("Einkauf");
    const garten = await client.createTaskList("Garten");
    await client.createTask({ title: "Rasen", listId: garten.id });
    render(App, { props: { client } });
    const user = userEvent.setup();
    const nav = await screen.findByRole("navigation", { name: "Navigation" });
    await fireEvent.contextMenu(await within(nav).findByRole("link", { name: /Garten/ }));
    await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Zusammenführen mit…" }));
    const targets = await screen.findByRole("menu", { name: "„Garten“ zusammenführen mit" });
    await user.click(within(targets).getByRole("menuitem", { name: "Einkauf" }));
    await waitFor(async () => expect((await client.listTaskLists()).map((l) => l.name)).toEqual(["Inbox", "Einkauf"]));
  });
});
