<script lang="ts">
  import type { SyncStatusDto } from "@tasks/core-client";
  import IconButton from "../../components/IconButton.svelte";
  import { Cloud, CloudOff, LogIn, RefreshCw, Settings, type IconComponent } from "../../icons";

  interface Props {
    status: SyncStatusDto;
    account: string | null;
    collapsed?: boolean;
    onlogin?: () => void;
    onsync?: () => void;
    /** Einstellungen (P5-05: aus der Kopfzeile hierher, gilt in beiden Modi). */
    onsettings?: () => void;
  }

  let { status, account, collapsed = false, onlogin, onsync, onsettings }: Props = $props();

  const pending = $derived(status.pendingOps);
  const view = $derived.by((): { icon: IconComponent; text: string; long?: string; tone: "muted" | "ok" | "warn" | "busy" } => {
    switch (status.state) {
      case "disabled":
        return { icon: CloudOff, text: "Nur lokal", long: "Nur auf diesem Gerät", tone: "muted" };
      case "syncing":
        return { icon: RefreshCw, text: "Synchronisiere …", tone: "busy" };
      case "idle":
        return pending > 0
          ? { icon: Cloud, text: `${pending} Änderung${pending === 1 ? "" : "en"} ausstehend`, tone: "muted" }
          : { icon: Cloud, text: "Synchronisiert", tone: "ok" };
      case "offline":
        return { icon: CloudOff, text: pending > 0 ? `Offline · ${pending} ausstehend` : "Offline", tone: "warn" };
      case "authRequired":
        return { icon: CloudOff, text: "Anmeldung erforderlich", tone: "warn" };
      case "rateLimited":
        return { icon: Cloud, text: "Kurze Pause (Server drosselt)", tone: "warn" };
      case "newerFormat":
        return { icon: CloudOff, text: "App-Update nötig", tone: "warn" };
      default:
        return { icon: CloudOff, text: "Sync-Fehler", tone: "warn" };
    }
  });
  const needsLogin = $derived(status.state === "disabled" || status.state === "authRequired");
  const tooltip = $derived(
    [
      view.long ?? view.text,
      account,
      status.lastSuccess ? `Zuletzt synchronisiert: ${new Date(status.lastSuccess).toLocaleString("de-DE")}` : null,
      status.state !== "disabled" && pending > 0 ? `${pending} lokale Änderung${pending === 1 ? "" : "en"} ausstehend` : null,
    ]
      .filter(Boolean)
      .join("\n"),
  );
</script>

<div class="footer" class:collapsed>
  <button
    type="button"
    class="status {view.tone}"
    aria-label="Sync-Status: {view.long ?? view.text}"
    data-tooltip={tooltip}
    data-tooltip-side="top"
    onclick={needsLogin ? onlogin : onsync}
  >
    <span class="icon" class:spin={status.state === "syncing"}>
      <view.icon size={16} strokeWidth={1.75} aria-hidden="true" />
    </span>
    {#if !collapsed}
      <span class="text">
        <span class="primary">{view.text}</span>
        {#if account}<span class="account">{account}</span>{/if}
      </span>
    {/if}
  </button>
  {#if needsLogin && !collapsed && onlogin}
    <button type="button" class="login" onclick={onlogin}>
      {#if !onsettings}<LogIn size={14} strokeWidth={1.75} aria-hidden="true" />{/if}Anmelden
    </button>
  {/if}
  {#if onsettings}
    <IconButton label="Einstellungen" icon={Settings} size={28} tooltipSide={collapsed ? "right" : "top"} onclick={onsettings} />
  {/if}
</div>

<style>
  .footer {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-2) var(--space-3);
    border-top: 1px solid var(--color-border);
  }
  .collapsed {
    flex-direction: column;
    justify-content: center;
  }
  .status {
    display: flex;
    flex: 1;
    min-width: 0;
    align-items: center;
    gap: var(--space-2);
    padding: 6px var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }
  .collapsed .status {
    flex: none;
  }
  .status:hover {
    background: var(--color-bg-hover);
  }
  .icon {
    display: grid;
    place-items: center;
  }
  .ok .icon {
    color: var(--color-success);
  }
  .warn .icon {
    color: var(--color-warning);
  }
  .busy .icon {
    color: var(--color-accent);
  }
  .spin {
    animation: spin 1s linear infinite;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .primary,
  .account {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .account {
    font-size: var(--text-xs);
  }
  .login {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-accent-text);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }
  .login:hover {
    background: var(--color-bg-hover);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
</style>
