import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
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

describe("Quick-Add", () => {
  it("creates on Enter, parses the day word and stays focused", async () => {
    const { client, user } = await setup();
    const input = screen.getByRole("textbox", { name: "Neue Task" });
    await user.click(input);
    await user.keyboard("Zahnarzt anrufen morgen");
    expect(screen.getByText("Morgen")).toBeTruthy();
    await user.keyboard("{Enter}");
    await screen.findByRole("option", { name: /^Zahnarzt anrufen/ });
    expect(document.activeElement).toBe(input);
    expect((input as HTMLInputElement).value).toBe("");
    const page = await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 });
    const created = page.items.find((t) => t.title === "Zahnarzt anrufen")!;
    expect(created.due).toEqual({ kind: "date", date: isoDate(addDays(new Date(), 1)) });
  });

  it("adds to the open list and defaults to today in the Today view", async () => {
    const { client, work, user } = await setup();
    location.hash = `#/list/${work.id}`;
    await screen.findByRole("heading", { name: "Arbeit" });
    await user.click(screen.getByRole("textbox", { name: "Neue Task" }));
    await user.keyboard("Review{Enter}");
    await screen.findByRole("option", { name: /^Review/ });
    location.hash = "#/today";
    await screen.findByRole("heading", { name: "Heute" });
    await user.click(screen.getByRole("textbox", { name: "Neue Task" }));
    await user.keyboard("Anrufen{Enter}");
    await screen.findByRole("option", { name: /^Anrufen/ });
    const inList = await client.listTasks({ view: { kind: "list", listId: work.id } }, { limit: 10 });
    expect(inList.items.map((t) => t.title)).toEqual(["Review"]);
  });

  it("N-button focuses quick add", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("button", { name: /Neue Task/ }));
    expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Neue Task" }));
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
    await user.click(await screen.findByRole("button", { name: "Morgen" }));
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

  it("closes with the close button", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("option", { name: /^Bericht/ }));
    await user.click(await screen.findByRole("button", { name: "Schließen" }));
    expect(screen.queryByRole("region", { name: "Task-Details" })).toBeNull();
  });
});
