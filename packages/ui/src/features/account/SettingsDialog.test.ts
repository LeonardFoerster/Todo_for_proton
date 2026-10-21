import { CoreClientError, FakeCoreClient } from "@tasks/core-client";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});
afterEach(() => vi.useRealTimers());

async function setup(options: ConstructorParameters<typeof FakeCoreClient>[0] = {}) {
  const client = new FakeCoreClient({ loginPolls: 1, ...options });
  await client.ensureInbox();
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Eingang" });
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Einstellungen" }));
  const dialog = await screen.findByRole("dialog", { name: "Einstellungen" });
  return { client, user, dialog };
}

describe("Login & Sync-Status", () => {
  it("works without login and shows the local-only state", async () => {
    const { dialog } = await setup();
    expect(within(dialog).getByText(/Ohne Anmeldung funktioniert alles/)).toBeTruthy();
    expect(within(dialog).getByText("Demo-Anmeldung ohne Server.")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Sync-Status: Nur auf diesem Gerät/ })).toBeTruthy();
  });

  it("logs in via browser step, polls until done and shows the account", async () => {
    const { client, user, dialog } = await setup();
    const begin = vi.spyOn(client.auth, "beginLogin");
    await user.click(within(dialog).getByRole("button", { name: "Mit Demo anmelden" }));
    expect(begin).toHaveBeenCalledOnce();
    expect(await within(dialog).findByText("Im Browser anmelden")).toBeTruthy();
    // Kein automatisches Öffnen (ADR-038): Der Link steht zum Kopieren da.
    expect((within(dialog).getByLabelText("Anmeldelink") as HTMLInputElement).value).toBe("https://login.example.invalid/");
    expect(within(dialog).getByRole("button", { name: "Link kopieren" })).toBeTruthy();
    // Fake: erster Poll „pending“, zweiter „done“ (Intervall 1 s).
    expect(await within(dialog).findByText("demo@example.invalid", {}, { timeout: 4000 })).toBeTruthy();
    expect(within(dialog).getByText("Synchronisiert")).toBeTruthy();
    expect(await screen.findByRole("button", { name: /Sync-Status: Synchronisiert/ })).toBeTruthy();
  });

  it("cancels a pending login", async () => {
    const { client, user, dialog } = await setup({ loginPolls: 99 });
    const cancel = vi.spyOn(client.auth, "cancelLogin");
    await user.click(within(dialog).getByRole("button", { name: "Mit Demo anmelden" }));
    await within(dialog).findByText("Im Browser anmelden");
    await user.click(within(dialog).getByRole("button", { name: "Abbrechen" }));
    expect(cancel).toHaveBeenCalledOnce();
    expect(within(dialog).getByRole("button", { name: "Mit Demo anmelden" })).toBeTruthy();
  });

  it("explains failures in plain words", async () => {
    const { client, user, dialog } = await setup();
    vi.spyOn(client.auth, "beginLogin").mockRejectedValueOnce(new CoreClientError("network", "network: dns"));
    await user.click(within(dialog).getByRole("button", { name: "Mit Demo anmelden" }));
    expect((await within(dialog).findByRole("alert")).textContent).toContain("Keine Verbindung");
  });

  it("logs out after confirmation; local data stays", async () => {
    const { client, user, dialog } = await setup({ loginPolls: 0 });
    await client.createTask({ title: "Bleibt" });
    await user.click(within(dialog).getByRole("button", { name: "Mit Demo anmelden" }));
    await within(dialog).findByText("demo@example.invalid", {}, { timeout: 4000 });
    await user.click(within(dialog).getByRole("button", { name: "Abmelden" }));
    await user.click(within(dialog).getByRole("button", { name: "Abmelden" })); // Bestätigung
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Mit Demo anmelden" })).toBeTruthy());
    expect(await client.auth.current()).toBeNull();
    expect((await client.listTasks({ view: { kind: "inbox" } }, { limit: 5 })).items).toHaveLength(1);
  });

  it("reflects status changes from the sync engine in the sidebar", async () => {
    const { client, user } = await setup();
    await user.keyboard("{Escape}");
    const states = [
      { state: "syncing", label: /Synchronisiere/ },
      { state: "idle", label: /Synchronisiert/ },
      { state: "offline", label: /Offline · 3 ausstehend/ },
      { state: "authRequired", label: /Anmeldung erforderlich/ },
    ] as const;
    for (const { state, label } of states) {
      client.setStatus({ state, lastSuccess: "2026-10-18T10:00:00.000Z", pendingOps: state === "offline" ? 3 : 0, message: null });
      const button = await screen.findByRole("button", { name: new RegExp(`Sync-Status: ${label.source}`) });
      if (state === "idle") expect(button.getAttribute("data-tooltip")).toContain("Zuletzt synchronisiert");
    }
  });
});
