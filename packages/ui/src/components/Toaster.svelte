<script lang="ts">
  import { X } from "../icons";
  import { toasts } from "./toasts.svelte";
</script>

<div class="toaster" role="region" aria-label="Benachrichtigungen">
  {#each toasts.items as toast (toast.id)}
    <div
      class="toast"
      class:danger={toast.tone === "danger"}
      role="status"
      aria-live="polite"
      onmouseenter={() => toasts.pause(toast.id)}
      onmouseleave={() => toasts.resume(toast.id)}
      onfocusin={() => toasts.pause(toast.id)}
      onfocusout={() => toasts.resume(toast.id)}
    >
      <span class="message">{toast.message}</span>
      {#if toast.actionLabel}
        <button type="button" class="action" onclick={() => toasts.act(toast.id)}>{toast.actionLabel}</button>
      {/if}
      <button type="button" class="close" aria-label="Hinweis schließen" onclick={() => toasts.dismiss(toast.id)}>
        <X size={14} strokeWidth={2} aria-hidden="true" />
      </button>
    </div>
  {/each}
</div>

<style>
  .toaster {
    position: fixed;
    left: var(--space-4);
    bottom: var(--space-4);
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 40px;
    max-width: min(420px, calc(100vw - 2 * var(--space-4)));
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-bg-inverse);
    color: var(--color-text-inverse);
    box-shadow: var(--shadow-popover);
    font-size: var(--text-md);
    pointer-events: auto;
    animation: toast-in var(--duration-base) var(--ease);
  }
  .toast.danger {
    border-left: 3px solid var(--color-danger);
  }
  .message {
    flex: 1;
    padding: var(--space-1) 0;
  }
  .action {
    height: 28px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-accent-inverse);
    font: inherit;
    font-weight: var(--weight-semibold);
    cursor: pointer;
  }
  .action:hover {
    background: color-mix(in srgb, var(--color-text-inverse) 10%, transparent);
  }
  .close {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-inverse);
    opacity: 0.7;
    cursor: pointer;
  }
  .close:hover {
    opacity: 1;
    background: color-mix(in srgb, var(--color-text-inverse) 10%, transparent);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
</style>
