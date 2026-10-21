// N10-01: Sortieren und Filtern je Ansicht – im Core angewandt, pro Ansicht gemerkt, als Chips sichtbar.
import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { compareInView, type TaskLike } from "../task-list/view-match";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

const shown = () => within(screen.getByRole("listbox")).queryAllByRole("option").map((r) => r.getAttribute("aria-label")?.split(",")[0]);

async function setup() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  await client.createTask({ title: "banane", priority: "low", due: { kind: "date", date: "2026-11-02" } });
  await client.createTask({ title: "Apfel", priority: "high" });
  await client.createTask({ title: "cola", due: { kind: "date", date: "2026-11-01" } });
  const tag = await client.ensureTag("obst");
  await client.createTask({ title: "Dattel", priority: "high", tags: [tag.id] });
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /^banane/ });
  return { client, user: userEvent.setup() };
}

async function choose(user: ReturnType<typeof userEvent.setup>, name: string | RegExp) {
  const button = screen.getByRole("button", { name: "Sortieren und filtern" });
  if (button.getAttribute("aria-expanded") !== "true") await user.click(button);
  const popover = await screen.findByRole("dialog", { name: "Sortieren und filtern" });
  await user.click(within(popover).getByRole("radio", { name }));
}

describe("Sortieren und filtern (N10-01)", () => {
  it("sorts by title, due and priority; filters by priority and tag; chips clear them", async () => {
    const { user } = await setup();
    expect(shown()).toEqual(["banane", "Apfel", "cola", "Dattel"]);
    await choose(user, "Titel");
    await waitFor(() => expect(shown()).toEqual(["Apfel", "banane", "cola", "Dattel"]));
    await choose(user, "Fälligkeit");
    await waitFor(() => expect(shown()).toEqual(["cola", "banane", "Apfel", "Dattel"]));
    await choose(user, "Priorität");
    await waitFor(() => expect(shown()).toEqual(["Apfel", "Dattel", "banane", "cola"]));
    await choose(user, "Hoch");
    await waitFor(() => expect(shown()).toEqual(["Apfel", "Dattel"]));
    await choose(user, "#obst");
    await waitFor(() => expect(shown()).toEqual(["Dattel"]));
    await user.keyboard("{Escape}");

    const chips = screen.getByRole("group", { name: "Aktive Sortierung und Filter" });
    expect(within(chips).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual([
      "Sortiert: Priorität aufheben",
      "Priorität: Hoch aufheben",
      "#obst aufheben",
    ]);
    await user.click(within(chips).getByRole("button", { name: "#obst aufheben" }));
    await user.click(within(screen.getByRole("group", { name: "Aktive Sortierung und Filter" })).getByRole("button", { name: "Priorität: Hoch aufheben" }));
    await waitFor(() => expect(shown()).toEqual(["Apfel", "Dattel", "banane", "cola"]));
  });

  it("remembers the options per view", async () => {
    const { user } = await setup();
    await choose(user, "Titel");
    await waitFor(() => expect(shown()[0]).toBe("Apfel"));
    await user.keyboard("{Escape}");
    await user.click(within(screen.getByRole("navigation", { name: "Navigation" })).getByRole("link", { name: /^Alle Tasks/ }));
    await waitFor(() => expect(shown()).toEqual(["banane", "Apfel", "cola", "Dattel"]));
    expect(screen.queryByRole("group", { name: "Aktive Sortierung und Filter" })).toBeNull();
    await user.click(within(screen.getByRole("navigation", { name: "Navigation" })).getByRole("link", { name: /^Eingang/ }));
    await waitFor(() => expect(shown()).toEqual(["Apfel", "banane", "cola", "Dattel"]));
  });

  it("client-side order matches the core rules", () => {
    const base = { listId: "l", status: "open" as const, completedAt: null, trashedAt: null, tags: [] as string[] };
    const t = (id: string, title: string, priority: TaskLike["priority"], due: TaskLike["due"], sortKey: string): TaskLike => ({
      ...base,
      id,
      title,
      priority,
      due,
      sortKey,
    });
    const items = [
      t("1", "b", "low", { kind: "date", date: "2026-11-02" }, "a1"),
      t("2", "A", "high", null, "a2"),
      t("3", "c", "none", { kind: "dateTime", at: "2026-11-01T12:00:00.000Z", tz: "Europe/Berlin" }, "a3"),
    ];
    const order = (sort: "due" | "priority" | "title") => [...items].sort((a, b) => compareInView(a, b, { view: "all" }, sort)).map((x) => x.id);
    expect(order("title")).toEqual(["2", "1", "3"]);
    expect(order("due")).toEqual(["3", "1", "2"]);
    expect(order("priority")).toEqual(["2", "1", "3"]);
  });
});
