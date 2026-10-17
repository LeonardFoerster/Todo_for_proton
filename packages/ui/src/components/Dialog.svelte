<script lang="ts">
  /** Modaler Dialog auf Basis von `<dialog>` (Fokusfalle, Esc, Hintergrund inert). */
  import type { Snippet } from "svelte";
  import { onMount } from "svelte";
  import { X } from "../icons";
  import IconButton from "./IconButton.svelte";

  interface Props {
    title: string;
    onclose: () => void;
    children: Snippet;
    width?: number;
  }

  let { title, onclose, children, width = 480 }: Props = $props();
  let dialog: HTMLDialogElement;

  onMount(() => {
    // Ohne showModal (ältere Engines, jsdom) wenigstens sichtbar öffnen.
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    return () => (typeof dialog.close === "function" ? dialog.close() : dialog.removeAttribute("open"));
  });
</script>

<dialog
  bind:this={dialog}
  style:--width="{width}px"
  aria-labelledby="dialog-title"
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
  onclick={(e) => {
    if (e.target === dialog) onclose();
  }}
>
  <header>
    <h2 id="dialog-title">{title}</h2>
    <IconButton label="Schließen" shortcut="Esc" icon={X} size={28} onclick={onclose} />
  </header>
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
  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }
</style>
