// N7-03: „Erinnerungen“ in den Einstellungen und Aktionen der Shell (Erinnerung öffnen, Tray „Neue Task“).
import { CoreClientError, FakeCoreClient, type BackgroundSettingsDto, type ShellAction } from "@tasks/core-client";
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

function withBackground(client: FakeCoreClient) {
  let stored: BackgroundSettingsDto = { reminders: true, allDayTime: "09:00", background: false, autostart: false };
  const listeners: ((a: ShellAction) => void)[] = [];
  const saved: BackgroundSettingsDto[] = [];
  client.background = {
    get: async () => stored,
    set: async (s) => {
      if (!/^\d{2}:\d{2}$/.test(s.allDayTime)) throw new CoreClientError("validation", "invalid time");
      saved.push(s);
      stored = s;
      return s;
    },
    onShellAction: (cb) => {
      listeners.push(cb);
      return () => listeners.splice(listeners.indexOf(cb), 1);
    },
  };
  return { saved, fire: (a: ShellAction) => listeners.forEach((l) => l(a)) };
}

describe("Erinnerungen", () => {
  it("saves each change; autostart needs background mode", async () => {
    const client = new FakeCoreClient();
    const { saved } = withBackground(client);
    await client.ensureInbox();
    render(App, { props: { client } });
    await screen.findByRole("heading", { name: "Eingang" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Einstellungen" }));
    const dialog = await screen.findByRole("dialog", { name: "Einstellungen" });
    await within(dialog).findByRole("heading", { name: "Erinnerungen" });
    const autostart = within(dialog).getByRole("checkbox", { name: /Mit dem System starten/ });
    expect((autostart as HTMLInputElement).disabled).toBe(true);
    await user.click(within(dialog).getByRole("checkbox", { name: /Im Hintergrund weiterlaufen/ }));
    await waitFor(() => expect(saved.at(-1)).toMatchObject({ background: true }));
    await user.click(within(dialog).getByRole("checkbox", { name: /Mit dem System starten/ }));
    await waitFor(() => expect(saved.at(-1)).toMatchObject({ background: true, autostart: true }));

    const time = within(dialog).getByRole("textbox", { name: "Uhrzeit für ganztägige" });
    await user.clear(time);
    await user.type(time, "7:30{Enter}");
    await waitFor(() => expect(saved.at(-1)).toMatchObject({ allDayTime: "07:30" }));
    await user.clear(time);
    await user.type(time, "25:00{Enter}");
    expect(within(dialog).getByRole("alert").textContent).toContain("Uhrzeit wie 09:00");
  });

  it("opens the task of a clicked reminder and the new-task window from the tray", async () => {
    const client = new FakeCoreClient();
    const { fire } = withBackground(client);
    await client.ensureInbox();
    const task = await client.createTask({ title: "Zahnarzt" });
    render(App, { props: { client } });
    await screen.findByRole("option", { name: /^Zahnarzt/ });
    fire({ kind: "openTask", taskId: task.id });
    const detail = await screen.findByRole("region", { name: "Task-Details" });
    await waitFor(() => expect((within(detail).getByRole("textbox", { name: "Titel" }) as HTMLTextAreaElement).value).toBe("Zahnarzt"));
    fire({ kind: "newTask" });
    expect(await screen.findByRole("dialog", { name: "Neue Task" })).toBeTruthy();
  });
});
