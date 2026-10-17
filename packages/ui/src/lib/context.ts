import type { CoreClient } from "@tasks/core-client";
import { getContext, setContext } from "svelte";

const CORE_CLIENT = Symbol("core-client");

/** Einmal in `App.svelte`; alle Container holen den Client darüber. */
export function setCoreClient(client: CoreClient): void {
  setContext(CORE_CLIENT, client);
}

export function useCoreClient(): CoreClient {
  const client = getContext<CoreClient | undefined>(CORE_CLIENT);
  if (!client) throw new Error("CoreClient fehlt – App.svelte setzt ihn per setCoreClient()");
  return client;
}
