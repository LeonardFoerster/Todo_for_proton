import { CoreClientError, FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function setup(count = 3) {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  for (let i = 1; i <= count; i++) await client.createTask({ title: `Task ${i}` });
  render(App, { props: { client } });
  await screen.findByRole("option", { name: /^Task 1$/ });
  return { client, user: userEvent.setup() };
}

const rows = () => screen.queryAllByRole("option");
const titles = () => rows().map((r) => r.getAttribute("aria-label")?.replace(/, erledigt$/, ""));
const checkbox = (title: string) => screen.getByRole("checkbox", { name: `Erledigt: ${title}` });

describe("TaskList", () => {
  it("names the priority at the end of the row label and on the check, without a flag (P5-03)", async () => {
    const client = new FakeCoreClient();
    await client.ensureInbox();
    await client.createTask({ title: "Wichtig", priority: "high", notes: "x" });
    render(App, { props: { client } });
    const row = await screen.findByRole("option", { name: /^Wichtig/ });
    expect(row.getAttribute("aria-label")).toBe("Wichtig, hohe Priorität");
    expect(within(row).getByRole("checkbox").getAttribute("data-tooltip")).toBe("Hohe Priorität");
    expect(within(row).getByLabelText("Hat Notizen")).toBeTruthy();
    expect(within(row).queryByLabelText("Hohe Priorität")).toBeNull();
  });

  it("renders open tasks of the view in order", async () => {
    await setup(3);
    expect(rows().map((r) => r.textContent?.trim())).toEqual(["Task 1", "Task 2", "Task 3"]);
  });

  it("checks off optimistically, then the row leaves the view; undo brings it back at its place", async () => {
    const { client, user } = await setup(3);
    const spy = vi.spyOn(client, "completeTask");
    await user.click(checkbox("Task 2"));
    // Sofort abgehakt (vor der Antwort); die Zeile bleibt für die Haken-Animation kurz stehen.
    expect(screen.getByRole("checkbox", { name: "Wieder öffnen: Task 2" }).getAttribute("aria-checked")).toBe("true");
    expect(rows()).toHaveLength(3);
    expect(spy).toHaveBeenCalledWith(expect.any(String), true);
    await waitFor(() => expect(titles()).toEqual(["Task 1", "Task 3"]));
    const toast = await screen.findByRole("status");
    expect(toast.textContent).toContain("„Task 2“ erledigt");

    await user.click(within(toast).getByRole("button", { name: "Rückgängig" }));
    await waitFor(() => expect(titles()).toEqual(["Task 1", "Task 2", "Task 3"]));
    const page = await client.listTasks({ view: { kind: "inbox" }, status: "open" }, { limit: 10 });
    expect(page.items).toHaveLength(3);
  });

  it("only shows completed tasks under Erledigt", async () => {
    const { user } = await setup(2);
    await user.click(checkbox("Task 1"));
    await waitFor(() => expect(titles()).toEqual(["Task 2"]));
    location.hash = "#/completed";
    await screen.findByRole("heading", { name: "Erledigt" });
    await waitFor(() => expect(titles()).toEqual(["Task 1"]));
    // Wieder öffnen in „Erledigt“: verlässt diese Ansicht.
    await user.click(screen.getByRole("checkbox", { name: "Wieder öffnen: Task 1" }));
    await waitFor(() => expect(rows()).toHaveLength(0));
  });

  it("undo during the animation keeps the row", async () => {
    const { client, user } = await setup(2);
    await user.click(checkbox("Task 1"));
    const id = (await client.listTasks({ view: { kind: "inbox" } }, { limit: 10 })).items[0].id;
    await client.completeTask(id, false);
    await new Promise((r) => setTimeout(r, 400));
    expect(titles()).toEqual(["Task 1", "Task 2"]);
  });

  it("Space completes the cursor row and moves on, so it can be pressed repeatedly", async () => {
    const { client, user } = await setup(3);
    await user.keyboard("{ArrowDown}");
    await user.keyboard(" ");
    expect(screen.getByRole("option", { name: "Task 2" }).getAttribute("aria-selected")).toBe("true");
    await user.keyboard(" ");
    await waitFor(() => expect(titles()).toEqual(["Task 3"]));
    const open = await client.listTasks({ view: { kind: "inbox" }, status: "open" }, { limit: 10 });
    expect(open.items.map((t) => t.title)).toEqual(["Task 3"]);
  });

  it("rolls back when saving fails", async () => {
    const { client, user } = await setup(1);
    vi.spyOn(client, "completeTask").mockRejectedValueOnce(new CoreClientError("storage", "disk full"));
    await user.click(checkbox("Task 1"));
    await waitFor(() => expect(checkbox("Task 1").getAttribute("aria-checked")).toBe("false"));
    expect((await screen.findByRole("status")).textContent).toContain("disk full");
    await new Promise((r) => setTimeout(r, 400));
    expect(rows(), "fehlgeschlagen: Zeile bleibt").toHaveLength(1);
  });

  it("applies changes from elsewhere by id (sync, other views)", async () => {
    const { client } = await setup(2);
    const created = await client.createTask({ title: "Von woanders" });
    await screen.findByRole("option", { name: /Von woanders/ });
    await client.updateTask(created.id, { title: "Umbenannt" });
    await screen.findByRole("option", { name: /Umbenannt/ });
    const work = await client.createTaskList("Arbeit");
    await client.moveTask(created.id, { listId: work.id });
    await waitFor(() => expect(screen.queryByRole("option", { name: /Umbenannt/ })).toBeNull());
    expect(rows()).toHaveLength(2);
  });

  it("virtualizes 1 000 tasks", async () => {
    await setup(1000);
    // 720 px Höhe / 36 px + Puffer statt 1 000 DOM-Zeilen.
    expect(rows().length).toBeLessThan(60);
    await waitFor(() => expect(screen.getByRole("link", { name: /Eingang/ }).textContent).toContain("999+"));
  });
});
