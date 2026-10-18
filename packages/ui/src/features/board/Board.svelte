<script lang="ts">
  /** Kanban-Board aller Listen (P2-06). Platzhalter bis dahin. */
  import { onMount } from "svelte";

  interface Props {
    /** Erste Darstellung steht (Startzeit, Sync-Start) – wie bei der Liste. */
    onready?: () => void;
  }

  let { onready }: Props = $props();

  onMount(() => requestAnimationFrame(() => onready?.()));
</script>

<div class="board" role="region" aria-label="Kanban-Board"></div>

<style>
  .board {
    height: 100%;
    border-top: 1px solid var(--color-border);
  }
</style>
