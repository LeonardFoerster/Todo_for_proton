<script lang="ts">
  /**
   * Einstellungen (lazy geladen): Konto & Sync, Darstellung, Tastatur. Der Login ist generisch nach
   * `AuthStepDto` gebaut – Browser öffnen → warten → fertig; Passwort, 2FA und Human Verification
   * erledigt die Login-Seite des Anbieters im Browser (ADR-013). Die UI kennt den Anbieter nur als Namen.
   */
  import { CoreClientError, type AuthInfoDto } from "@tasks/core-client";
  import { onDestroy, onMount } from "svelte";
  import Button from "../../components/Button.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import ThemeSwitch from "../../components/ThemeSwitch.svelte";
  import { toasts } from "../../components/toasts.svelte";
  import { Cloud, CloudOff, Copy, ExternalLink, Keyboard, LogIn, LogOut, RefreshCw, ShieldCheck } from "../../icons";
  import { useCoreClient } from "../../lib/context";
  import type { SyncState } from "../../lib/sync.svelte";

  interface Props {
    sync: SyncState;
    onclose: () => void;
    onshortcuts: () => void;
  }

  let { sync, onclose, onshortcuts }: Props = $props();

  const client = useCoreClient();

  type Step =
    | { kind: "idle" }
    | { kind: "starting" }
    | { kind: "waiting"; flow: string; url: string; pollMs: number; expiresAt: number }
    | { kind: "error"; message: string };

  let info = $state<AuthInfoDto | null>(null);
  let step = $state<Step>({ kind: "idle" });
  let confirmLogout = $state(false);
  let now = $state(Date.now());
  let pollTimer: ReturnType<typeof setTimeout> | undefined;
  let clock: ReturnType<typeof setInterval> | undefined;

  const DATE = new Intl.DateTimeFormat("de-DE", { dateStyle: "medium", timeStyle: "short" });
  const status = $derived(sync.status);
  const secondsLeft = $derived(step.kind === "waiting" ? Math.max(0, Math.round((step.expiresAt - now) / 1000)) : 0);

  onMount(async () => {
    try {
      info = await client.auth.info();
    } catch {
      info = { provider: "", notice: "", available: false };
    }
  });

  onDestroy(() => {
    stopWaiting();
    // Offenen Login-Vorgang beim Schließen verwerfen.
    if (step.kind === "waiting") void client.auth.cancelLogin(step.flow).catch(() => {});
  });

  function stopWaiting() {
    clearTimeout(pollTimer);
    clearInterval(clock);
  }

  function message(e: unknown): string {
    const err = CoreClientError.from(e);
    if (err.kind === "network") return "Keine Verbindung zum Anmeldeserver. Prüfe die Internetverbindung.";
    if (err.kind === "auth" && /expired/.test(err.message)) return "Die Anmeldung ist abgelaufen. Bitte erneut starten.";
    if (err.kind === "auth" && /cancel/.test(err.message)) return "Die Anmeldung wurde abgebrochen.";
    if (err.kind === "unavailable") return err.message;
    return `Anmeldung fehlgeschlagen: ${err.message}`;
  }

  async function begin() {
    step = { kind: "starting" };
    try {
      const next = await client.auth.beginLogin();
      if (next.kind !== "openBrowser") throw new CoreClientError("internal", "unerwarteter Login-Schritt");
      step = {
        kind: "waiting",
        flow: next.flow,
        url: next.url,
        pollMs: next.pollIntervalS * 1000,
        expiresAt: Date.now() + next.expiresInS * 1000,
      };
      clock = setInterval(() => (now = Date.now()), 1000);
      pollTimer = setTimeout(poll, step.pollMs);
    } catch (e) {
      step = { kind: "error", message: message(e) };
    }
  }

  async function poll() {
    if (step.kind !== "waiting") return;
    const { flow, pollMs } = step;
    try {
      const next = await client.auth.pollLogin(flow);
      if (step.kind !== "waiting" || step.flow !== flow) return; // inzwischen abgebrochen
      if (next.kind === "done") {
        stopWaiting();
        step = { kind: "idle" };
        await sync.refreshAccount();
        toasts.show({ message: `Angemeldet als ${next.session.accountLabel}` });
        return;
      }
      pollTimer = setTimeout(poll, pollMs);
    } catch (e) {
      stopWaiting();
      step = { kind: "error", message: message(e) };
    }
  }

  async function cancel() {
    if (step.kind !== "waiting") return;
    const { flow } = step;
    stopWaiting();
    step = { kind: "idle" };
    await client.auth.cancelLogin(flow).catch(() => {});
  }

  async function copyLink(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      toasts.show({ message: "Link kopiert" });
    } catch {
      toasts.show({ message: "Kopieren nicht möglich – Link im Browser öffnen", tone: "danger" });
    }
  }

  async function logout() {
    confirmLogout = false;
    try {
      await client.auth.logout();
      await sync.refreshAccount();
      toasts.show({ message: "Abgemeldet. Deine Tasks bleiben auf diesem Gerät." });
    } catch (e) {
      toasts.show({ message: `Abmelden fehlgeschlagen: ${(e as Error).message}`, tone: "danger" });
    }
  }

  const stateText: Record<string, string> = {
    disabled: "Nicht angemeldet – Tasks bleiben auf diesem Gerät.",
    idle: "Synchronisiert",
    syncing: "Synchronisiere …",
    offline: "Offline – Änderungen werden nachgeholt.",
    authRequired: "Anmeldung erforderlich – die Sitzung ist abgelaufen.",
    rateLimited: "Der Server drosselt gerade – Sync pausiert kurz.",
    newerFormat: "Die Daten stammen von einer neueren App-Version. Bitte aktualisieren.",
    error: "Sync-Fehler – wird automatisch erneut versucht.",
  };
</script>

<Dialog title="Einstellungen" {onclose} width={520}>
  <section aria-labelledby="sync-title">
    <h3 id="sync-title">Konto & Synchronisation</h3>

    {#if sync.account}
      <div class="card">
        <div class="account">
          <span class="avatar" aria-hidden="true">{sync.account.slice(0, 1).toUpperCase()}</span>
          <div class="who">
            <strong>{sync.account}</strong>
            <span class="muted">{info?.provider ? `Angemeldet bei ${info.provider}` : "Angemeldet"}</span>
          </div>
        </div>
        <dl class="facts">
          <dt>Status</dt>
          <dd class="state {status.state}">
            {#if status.state === "syncing"}<RefreshCw size={14} strokeWidth={2} class="spin" aria-hidden="true" />
            {:else if status.state === "idle"}<Cloud size={14} strokeWidth={2} aria-hidden="true" />
            {:else}<CloudOff size={14} strokeWidth={2} aria-hidden="true" />{/if}
            {stateText[status.state]}
          </dd>
          <dt>Zuletzt</dt>
          <dd>{status.lastSuccess ? DATE.format(new Date(status.lastSuccess)) : "noch nie"}</dd>
          <dt>Ausstehend</dt>
          <dd>{status.pendingOps === 0 ? "keine Änderungen" : `${status.pendingOps} Änderung${status.pendingOps === 1 ? "" : "en"}`}</dd>
        </dl>
        {#if status.message}<p class="detail">{status.message}</p>{/if}
        <div class="actions">
          <Button icon={RefreshCw} onclick={() => void client.sync.now()} disabled={status.state === "syncing"}>
            Jetzt synchronisieren
          </Button>
          {#if status.state === "authRequired"}
            <Button variant="primary" icon={LogIn} onclick={() => void begin()}>Erneut anmelden</Button>
          {/if}
          {#if confirmLogout}
            <span class="confirm">
              Wirklich abmelden?
              <Button variant="danger" onclick={() => void logout()}>Abmelden</Button>
              <Button variant="ghost" onclick={() => (confirmLogout = false)}>Abbrechen</Button>
            </span>
          {:else}
            <Button variant="ghost" icon={LogOut} onclick={() => (confirmLogout = true)}>Abmelden</Button>
          {/if}
        </div>
      </div>
    {:else if step.kind === "waiting"}
      <div class="card waiting" role="status" aria-live="polite">
        <span class="pulse" aria-hidden="true"><ExternalLink size={20} strokeWidth={1.75} /></span>
        <strong>Im Browser anmelden</strong>
        <p class="muted">
          Wir haben die Anmeldeseite von {info?.provider || "deinem Anbieter"} im Browser geöffnet. Melde dich dort an
          und bestätige den Zugriff – Zwei-Faktor-Code und weitere Prüfungen erledigst du direkt dort.
        </p>
        <p class="countdown">Warte auf Bestätigung … noch {Math.floor(secondsLeft / 60)}:{String(secondsLeft % 60).padStart(2, "0")} min</p>
        <div class="actions">
          <Button icon={ExternalLink} onclick={() => void client.auth.openBrowser((step as { flow: string }).flow)}>
            Erneut öffnen
          </Button>
          <Button variant="ghost" icon={Copy} onclick={() => void copyLink((step as { url: string }).url)}>Link kopieren</Button>
          <Button variant="ghost" onclick={() => void cancel()}>Abbrechen</Button>
        </div>
      </div>
    {:else}
      <div class="card">
        <p>
          Melde dich an, um deine Tasks Ende-zu-Ende-verschlüsselt zwischen deinen Geräten abzugleichen. Ohne Anmeldung
          funktioniert alles – die Daten bleiben dann nur auf diesem Gerät.
        </p>
        {#if step.kind === "error"}<p class="error" role="alert">{step.message}</p>{/if}
        {#if info && !info.available}
          <p class="error">Anmeldung auf diesem System nicht möglich: Es wurde kein Schlüsselbund (Secret Service) gefunden.</p>
        {/if}
        <div class="actions">
          <Button
            variant="primary"
            icon={LogIn}
            disabled={!info?.available || step.kind === "starting"}
            onclick={() => void begin()}
          >
            {info?.provider ? `Mit ${info.provider} anmelden` : "Anmelden"}
          </Button>
        </div>
        {#if info?.notice}
          <p class="notice"><ShieldCheck size={14} strokeWidth={1.75} aria-hidden="true" />{info.notice}</p>
        {/if}
      </div>
    {/if}
  </section>

  <section aria-labelledby="look-title">
    <h3 id="look-title">Darstellung</h3>
    <ThemeSwitch />
  </section>

  <section aria-labelledby="keys-title">
    <h3 id="keys-title">Tastatur</h3>
    <Button variant="ghost" icon={Keyboard} onclick={onshortcuts}>Alle Tastaturkürzel anzeigen</Button>
  </section>
</Dialog>

<style>
  section + section {
    margin-top: var(--space-6);
  }
  h3 {
    margin: 0 0 var(--space-3);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .card {
    padding: var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
  }
  .card p {
    margin: 0 0 var(--space-3);
    line-height: 1.5;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .account {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-full);
    background: var(--color-accent-fill);
    color: var(--color-on-accent);
    font-weight: var(--weight-semibold);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .who strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .who .muted {
    font-size: var(--text-sm);
  }
  .facts {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: var(--space-2) var(--space-3);
    margin: var(--space-4) 0;
    font-size: var(--text-sm);
  }
  .facts dt {
    color: var(--color-text-muted);
  }
  .facts dd {
    margin: 0;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .state.idle :global(svg) {
    color: var(--color-success);
  }
  .state.offline :global(svg),
  .state.authRequired :global(svg),
  .state.error :global(svg),
  .state.rateLimited :global(svg),
  .state.newerFormat :global(svg) {
    color: var(--color-warning);
  }
  .state :global(.spin) {
    color: var(--color-accent);
    animation: spin 1s linear infinite;
  }
  .detail {
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  .confirm {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }
  .waiting {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
  }
  .waiting strong {
    margin: var(--space-2) 0 var(--space-1);
    font-size: var(--text-lg);
  }
  .pulse {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-full);
    background: var(--color-bg-selected);
    color: var(--color-accent-text);
  }
  .countdown {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }
  .error {
    color: var(--color-danger);
    font-size: var(--text-sm);
  }
  .notice {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: var(--space-3) 0 0 !important;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }
  .notice :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .state :global(.spin) {
      animation: none;
    }
  }
</style>
