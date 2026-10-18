import { devSelftest, TauriCoreClient } from "@tasks/core-client/tauri";
import { App } from "@tasks/ui";
import { mount } from "svelte";

const client = new TauriCoreClient();
mount(App, { target: document.getElementById("app")!, props: { client } });

// Netzwerkwechsel der WebView an den Sync-Scheduler (die Shell hat keinen eigenen Monitor).
addEventListener("online", () => void client.setOnline(true));
addEventListener("offline", () => void client.setOnline(false));

// `runtime`: Suiten aus `TASKS_SELFTEST` der Shell, Bericht auf deren stdout, danach beenden
// (`tools/acceptance-two-devices.sh`).
const selftest = import.meta.env.VITE_TASKS_SELFTEST;
if (selftest) {
  void import("./selftest").then(async (m) => {
    const runtime = selftest === "runtime";
    const suites = runtime ? await devSelftest.suites() : selftest;
    if (suites) setTimeout(() => void m.runSelftest(client, suites, runtime ? devSelftest.report : undefined), 500);
  });
}
