// P5-09: schmale Ansicht – Kopfzeilen-Aktionen im Menü, „Neue Task“ als runder Button unten.
import { FakeCoreClient } from "@tasks/core-client";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import App from "../../App.svelte";
import { toasts } from "../../components/toasts.svelte";
import { density } from "../../lib/density.svelte";

beforeEach(() => {
  location.hash = "";
  localStorage.clear();
  toasts.items = [];
  density.narrow = true;
});
afterEach(() => {
  density.narrow = false;
});

describe("narrow window", () => {
  it("moves the header icons into one menu and offers a round „Neue Task“ button", async () => {
    const client = new FakeCoreClient();
    await client.ensureInbox();
    await client.createTask({ title: "Task 1" });
    render(App, { props: { client } });
    await screen.findByRole("option", { name: /^Task 1/ });
    const user = userEvent.setup();

    expect(screen.queryByRole("group", { name: "Darstellung" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Ansicht und Aktionen" }));
    const menu = await screen.findByRole("menu", { name: "Ansicht und Aktionen" });
    expect(within(menu).getByRole("menuitem", { name: /Auswählen/ })).toBeTruthy();
    await user.click(within(menu).getByRole("menuitem", { name: /Als Kanban anzeigen/ }));
    expect(localStorage.getItem("tasks.layout")).toBe("board");

    const fab = screen.getAllByRole("button", { name: "Neue Task" }).find((b) => b.classList.contains("fab"))!;
    await user.click(fab);
    expect(await screen.findByRole("dialog", { name: "Neue Task" })).toBeTruthy();
  });
});
