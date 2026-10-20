<script lang="ts">
  /**
   * Modaler Dialog auf Basis von `<dialog>` (Fokusfalle, Esc, Hintergrund inert). `bare`: ohne Kopf, der Titel
   * ist nur der zugängliche Name; `placement="top"`: fest oben verankert (wächst nur nach unten), im schmalen
   * Fenster ein Bereich, der unten aufgeht (P6-02).
   */
  import type { Snippet } from "svelte";
  import { onMount } from "svelte";
  import { X } from "../icons";
  import IconButton from "./IconButton.svelte";

  interface Props {
    title: string;
    onclose: () => void;
    children: Snippet;
    width?: number;
    bare?: boolean;
    placement?: "center" | "top";
  }

  let { title, onclose, children, width = 480, bare = false, placement = "center" }: Props = $props();
  /** Eindeutig je Dialog – verschachtelte Dialoge dürfen sich nicht dieselbe Titel-ID teilen. */
  const uid = $props.id();
  const titleId = `dialog-title-${uid}`;
  let dialog: HTMLDialogElement;

  onMount(() => {
    // Ohne showModal (ältere Engines, jsdom) wenigstens sichtbar öffnen.
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    // Fokus auf den Dialog selbst statt auf den ersten Button (sonst erscheint dessen Tooltip).
    dialog.focus();
    return () => (typeof dialog.close === "function" ? dialog.close() : dialog.removeAttribute("open"));
  });
</script>

<dialog
  bind:this={dialog}
  style:--width="{width}px"
  class:bare
  class:top={placement === "top"}
  aria-labelledby={bare ? undefined : titleId}
  aria-label={bare ? title : undefined}
  tabindex="-1"
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
  onclick={(e) => {
    if (e.target === dialog) onclose();
  }}
>
  {#if !bare}
    <header>
      <h2 id={titleId}>{title}</h2>
      <IconButton label="Schließen" shortcut="Esc" icon={X} size={28} onclick={onclose} />
    </header>
  {/if}
  <div class="content">{@render children()}</div>
</dialog>

<style>
  dialog {
    width: min(var(--width), calc(100vw - 32px));
    max-height: min(640px, calc(100vh - 32px));
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-bg-raised);
    color: var(--color-text);
    box-shadow: var(--shadow-popover);
    animation: dialog-in var(--duration-base) var(--ease);
  }
  dialog:focus {
    outline: none;
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-5);
    border-bottom: 1px solid var(--color-border);
  }
  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .content {
    padding: var(--space-4) var(--space-5) var(--space-5);
    overflow-y: auto;
  }
  .bare .content {
    padding: var(--space-4);
  }
  /* Oben verankert: wächst beim Aufklappen nur nach unten, statt neu zu zentrieren. */
  .top {
    margin: 15vh auto auto;
  }
  /* Schmal: Bereich von unten, volle Breite, über der Bildschirmtastatur (dvh). */
  @media (max-width: 699px) {
    .top {
      width: 100%;
      max-width: 100%;
      max-height: 85dvh;
      margin: auto 0 0;
      border-bottom: 0;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      padding-bottom: env(safe-area-inset-bottom, 0px);
      animation-name: sheet-in;
    }
  }
  @keyframes sheet-in {
    from {
      transform: translateY(24px);
      opacity: 0;
    }
  }
  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }
</style>
