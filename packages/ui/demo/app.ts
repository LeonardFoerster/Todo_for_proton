// Ganze App im Browser mit FakeCoreClient: http://localhost:5180/app.html (?n=1000 für viele Tasks,
// ?latency=50 für langsame Aufrufe).
import { FakeCoreClient } from "@tasks/core-client";
import { mount } from "svelte";
import { App, setThemePreference } from "../src";
import { seed } from "./seed";

const params = new URLSearchParams(location.search);
// Für Screenshots: ?theme=dark|light, ?collapsed=1
const theme = params.get("theme");
if (theme === "dark" || theme === "light") setThemePreference(theme);
if (params.has("collapsed")) localStorage.setItem("tasks.sidebar-collapsed", params.get("collapsed") === "1" ? "1" : "0");
const client = new FakeCoreClient({ latencyMs: Number(params.get("latency") ?? 0) });
await seed(client, Number(params.get("n") ?? 0));
mount(App, { target: document.getElementById("app")!, props: { client } });
