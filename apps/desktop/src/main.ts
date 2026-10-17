import { invoke } from "@tauri-apps/api/core";
import { mount } from "svelte";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });

// Netzwerkwechsel der WebView an den Sync-Scheduler (die Shell hat keinen eigenen Monitor).
addEventListener("online", () => invoke("set_online", { online: true }));
addEventListener("offline", () => invoke("set_online", { online: false }));
