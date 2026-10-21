// N10-04: Erste Schritte im leeren Eingang – ausblendbar (gemerkt), Einstieg „Erste Task anlegen“.
import { FakeCoreClient } from "@tasks/core-client";
import { cleanup, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
});

async function start() {
  const client = new FakeCoreClient();
  await client.ensureInbox();
  render(App, { props: { client } });
  await screen.findByRole("heading", { name: "Eingang", level: 1 });
  return { client, user: userEvent.setup() };
}

describe("Erste Schritte (N10-04)", () => {
  it("shows the welcome in the empty inbox, opens a new task and hides for good", async () => {
    const { user } = await start();
    const welcome = await screen.findByRole("region", { name: "Willkommen" });
    expect(welcome.textContent).toContain("#tag");
    await user.click(within(welcome).getByRole("button", { name: "Erste Task anlegen" }));
    expect(await screen.findByRole("dialog", { name: "Neue Task" })).toBeTruthy();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Neue Task" })).toBeNull());

    await user.click(within(screen.getByRole("region", { name: "Willkommen" })).getByRole("button", { name: "Ausblenden" }));
    expect(screen.queryByRole("region", { name: "Willkommen" })).toBeNull();
    expect(screen.getByText(/Alles, was keiner Liste zugeordnet ist/)).toBeTruthy();

    cleanup();
    await start();
    expect(screen.queryByRole("region", { name: "Willkommen" })).toBeNull();
  });
});
