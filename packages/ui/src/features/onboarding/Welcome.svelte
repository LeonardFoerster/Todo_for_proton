<script lang="ts">
  /**
   * Erste Schritte (N10-04): im leeren Eingang beim ersten Start – Texterkennung, Suche und Kürzel,
   * Anmelden für den Sync. „Ausblenden“ merkt sich das Gerät.
   */
  import Button from "../../components/Button.svelte";
  import { Keyboard, Plus, RefreshCw, Search } from "../../icons";

  interface Props {
    onnewtask?: () => void;
    onsettings?: () => void;
    ondismiss: () => void;
  }

  let { onnewtask, onsettings, ondismiss }: Props = $props();
</script>

<section class="welcome" aria-labelledby="welcome-title">
  <h2 id="welcome-title">Willkommen</h2>
  <ul>
    <li>
      <span class="icon"><Plus size={16} strokeWidth={1.75} aria-hidden="true" /></span>
      <span>
        <strong>Neue Task mit <kbd>N</kbd></strong> – im Titel erkannt:
        <code>morgen 14:00</code> · <code>!1</code> · <code>@Liste</code> · <code>#tag</code> · <code>jeden Mo</code>
      </span>
    </li>
    <li>
      <span class="icon"><Search size={16} strokeWidth={1.75} aria-hidden="true" /></span>
      <span><strong><kbd>Strg+K</kbd> sucht</strong> Tasks und Notizen und führt Befehle aus.</span>
    </li>
    <li>
      <span class="icon"><Keyboard size={16} strokeWidth={1.75} aria-hidden="true" /></span>
      <span><strong><kbd>?</kbd> zeigt alle Kürzel</strong> – fast alles geht ohne Maus.</span>
    </li>
    <li>
      <span class="icon"><RefreshCw size={16} strokeWidth={1.75} aria-hidden="true" /></span>
      <span>
        <strong>Auf mehreren Geräten?</strong> Anmelden synchronisiert Ende-zu-Ende-verschlüsselt; ohne
        Anmeldung bleibt alles auf diesem Gerät.
      </span>
    </li>
  </ul>
  <div class="actions">
    {#if onnewtask}<Button variant="primary" icon={Plus} onclick={onnewtask}>Erste Task anlegen</Button>{/if}
    {#if onsettings}<Button onclick={onsettings}>Anmelden…</Button>{/if}
    <Button variant="ghost" onclick={ondismiss}>Ausblenden</Button>
  </div>
</section>

<style>
  .welcome {
    width: min(520px, 100%);
    padding: var(--space-5) var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg-raised);
    color: var(--color-text);
    text-align: left;
  }
  h2 {
    margin: 0 0 var(--space-3);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0 0 var(--space-5);
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    line-height: 1.5;
  }
  li {
    display: flex;
    gap: var(--space-3);
  }
  .icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-md);
    background: var(--color-bg-selected);
    color: var(--color-accent-text);
  }
  strong {
    font-weight: var(--weight-semibold);
  }
  code,
  kbd {
    padding: 0 4px;
    border-radius: var(--radius-sm);
    background: var(--color-bg-hover);
    font-family: inherit;
    font-size: 0.95em;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
