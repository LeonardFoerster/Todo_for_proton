import type { CoreClient, SyncStatusDto } from "@tasks/core-client";

/** Sync-Status + angemeldetes Konto für Sidebar-Fußzeile und Einstellungen. */
export class SyncState {
  status = $state<SyncStatusDto>({ state: "disabled", lastSuccess: null, pendingOps: 0, message: null });
  account = $state<string | null>(null);

  constructor(private readonly client: CoreClient) {}

  start(): () => void {
    const off = this.client.onSyncStatus((s) => {
      this.status = s;
      // Nach Login/Logout kann sich das Konto geändert haben.
      void this.refreshAccount();
    });
    void this.client.sync.status().then((s) => (this.status = s));
    void this.refreshAccount();
    return off;
  }

  async refreshAccount() {
    try {
      this.account = (await this.client.auth.current())?.accountLabel ?? null;
    } catch {
      this.account = null;
    }
  }
}
