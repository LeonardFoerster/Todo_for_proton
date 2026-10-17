import { TauriCoreClient } from "@tasks/core-client/tauri";
import { App } from "@tasks/ui";
import { mount } from "svelte";

const client = new TauriCoreClient();
mount(App, { target: document.getElementById("app")!, props: { client } });

// Netzwerkwechsel der WebView an den Sync-Scheduler (die Shell hat keinen eigenen Monitor).
addEventListener("online", () => void client.setOnline(true));
addEventListener("offline", () => void client.setOnline(false));

if (import.meta.env.VITE_TASKS_SELFTEST) {
  void import("./selftest").then((m) => setTimeout(() => void m.runSelftest(client), 500));
}
