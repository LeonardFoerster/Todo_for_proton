// N6-04: „Daten“ in den Einstellungen – Export und Wiederherstellen (nur mit `client.data`, Desktop).
import { FakeCoreClient } from "@tasks/core-client";
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

async function open(client: FakeCoreClient) {
  await client.ensureInbox();
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Eingang" });
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Einstellungen" }));
  return { user, dialog: await screen.findByRole("dialog", { name: "Einstellungen" }) };
}

describe("Daten", () => {
  it("is hidden without shell support", async () => {
    const { dialog } = await open(new FakeCoreClient());
    expect(within(dialog).queryByRole("heading", { name: "Daten" })).toBeNull();
  });

  it("exports with the local date and reports where; restores and reports counts", async () => {
    const client = new FakeCoreClient();
    const exportAll = vi.fn(async () => ({ path: "/home/x/Tasks-Export-2026-10-21", lists: 2, tasks: 12, notes: 1 }));
    const importFile = vi.fn(async () => ({ listsCreated: 1, tasksCreated: 5, tasksSkipped: 7 }));
    client.data = { exportAll, importFile };
    const { user, dialog } = await open(client);
    await user.click(within(dialog).getByRole("button", { name: "Exportieren…" }));
    expect(exportAll).toHaveBeenCalledWith(expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/));
    await waitFor(() =>
      expect(screen.getByText("Exportiert: 12 Tasks, 2 Listen, 1 Notiz nach /home/x/Tasks-Export-2026-10-21")).toBeTruthy(),
    );
    await user.click(within(dialog).getByRole("button", { name: "Aus Export wiederherstellen…" }));
    await waitFor(() => expect(screen.getByText("5 Tasks wiederhergestellt, 1 Liste neu, 7 schon vorhanden")).toBeTruthy());
  });

  it("says nothing when the dialog was cancelled", async () => {
    const client = new FakeCoreClient();
    client.data = { exportAll: async () => null, importFile: async () => null };
    const { user, dialog } = await open(client);
    await user.click(within(dialog).getByRole("button", { name: "Exportieren…" }));
    await user.click(within(dialog).getByRole("button", { name: "Aus Export wiederherstellen…" }));
    expect(toasts.items).toHaveLength(0);
  });
});
