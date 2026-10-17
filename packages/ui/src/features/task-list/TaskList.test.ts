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
const checkbox = (title: string) => screen.getByRole("checkbox", { name: `Erledigt: ${title}` });

describe("TaskList", () => {
  it("renders open tasks of the view in order", async () => {
    await setup(3);
    expect(rows().map((r) => r.textContent?.trim())).toEqual(["Task 1", "Task 2", "Task 3"]);
  });

  it("checks off optimistically, keeps the row dimmed and offers undo", async () => {
    const { client, user } = await setup(2);
    const spy = vi.spyOn(client, "completeTask");
    await user.click(checkbox("Task 1"));
    // Sofort abgehakt (vor der Antwort), Zeile bleibt stehen.
    expect(screen.getByRole("checkbox", { name: "Wieder öffnen: Task 1" }).getAttribute("aria-checked")).toBe("true");
    expect(rows()).toHaveLength(2);
    expect(spy).toHaveBeenCalledWith(expect.any(String), true);
    const toast = await screen.findByRole("status");
    expect(toast.textContent).toContain("„Task 1“ erledigt");
    await user.click(within(toast).getByRole("button", { name: "Rückgängig" }));
    await waitFor(() => expect(checkbox("Task 1").getAttribute("aria-checked")).toBe("false"));
    const page = await client.listTasks({ view: { kind: "inbox" }, status: "open" }, { limit: 10 });
    expect(page.items).toHaveLength(2);
  });

  it("rolls back when saving fails", async () => {
    const { client, user } = await setup(1);
    vi.spyOn(client, "completeTask").mockRejectedValueOnce(new CoreClientError("storage", "disk full"));
    await user.click(checkbox("Task 1"));
    await waitFor(() => expect(checkbox("Task 1").getAttribute("aria-checked")).toBe("false"));
    expect((await screen.findByRole("status")).textContent).toContain("disk full");
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
    expect(screen.getByRole("link", { name: /Eingang/ }).textContent).toContain("999+");
  });
});
