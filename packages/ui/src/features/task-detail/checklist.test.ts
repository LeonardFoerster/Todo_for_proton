// N8-01: Checkliste im Detail – hinzufügen, abhaken, umbenennen, Enter/Rücktaste, verschieben; „2/5“ in der Zeile.
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
  const task = await client.createTask({ title: "Einkauf" });
  render(App, { props: { client } });
  const user = userEvent.setup();
  await user.click(await screen.findByRole("option", { name: /^Einkauf/ }));
  const detail = await screen.findByRole("region", { name: "Task-Details" });
  return { client, task, user, detail };
}

const texts = async (client: FakeCoreClient, id: string) =>
  (await client.getTask(id)).checklist.map((i) => `${i.done ? "x" : "-"} ${i.text}`);

describe("Checkliste (N8-01)", () => {
  it("adds, ticks, renames, inserts with Enter and deletes with Backspace", async () => {
    const { client, task, user, detail } = await setup();
    const add = within(detail).getByRole("textbox", { name: "Punkt hinzufügen" });
    await user.type(add, "Brot{Enter}");
    await user.type(add, "Milch{Enter}");
    await waitFor(async () => expect(await texts(client, task.id)).toEqual(["- Brot", "- Milch"]));

    await user.click(within(detail).getByRole("checkbox", { name: "Erledigt: Brot" }));
    await waitFor(async () => expect(await texts(client, task.id)).toEqual(["x Brot", "- Milch"]));
    expect(within(detail).getByText("1 von 2 erledigt")).toBeTruthy();
    await waitFor(() => expect(screen.getByRole("option", { name: /^Einkauf/ }).textContent).toContain("1/2"));

    // Umbenennen + Enter öffnet einen neuen Punkt darunter.
    const first = within(detail).getByRole("textbox", { name: "Punkt 1" });
    await user.clear(first);
    await user.type(first, "Vollkornbrot{Enter}");
    const fresh = await within(detail).findByRole("textbox", { name: "Neuer Punkt" });
    await waitFor(() => expect(document.activeElement).toBe(fresh));
    await user.type(fresh, "Butter{Enter}");
    await waitFor(async () => expect(await texts(client, task.id)).toEqual(["x Vollkornbrot", "- Butter", "- Milch"]));

    // Leerer neuer Punkt wird verworfen; Rücktaste im geleerten Punkt löscht ihn.
    await user.keyboard("{Escape}");
    const butter = within(detail).getByRole("textbox", { name: "Punkt 2" });
    await user.clear(butter);
    await user.keyboard("{Backspace}");
    await waitFor(async () => expect(await texts(client, task.id)).toEqual(["x Vollkornbrot", "- Milch"]));
  });

  it("reorders with Alt+↑ and deletes with the button; search finds item text", async () => {
    const { client, task, user, detail } = await setup();
    await client.updateTask(task.id, {
      checklist: ["A", "B", "C"].map((text) => ({ id: "", text, done: false })),
    });
    const c = await within(detail).findByRole("textbox", { name: "Punkt 3" });
    c.focus();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    await waitFor(async () => expect(await texts(client, task.id)).toEqual(["- A", "- C", "- B"]));
    await user.click(within(detail).getByRole("button", { name: "Punkt löschen: A" }));
    await waitFor(async () => expect(await texts(client, task.id)).toEqual(["- C", "- B"]));
    expect((await client.searchTasks("C", 10)).map((t) => t.title)).toEqual(["Einkauf"]);
  });
});
