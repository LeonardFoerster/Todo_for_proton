<script lang="ts">
  import { followSystemTheme, ThemeSwitch, Toaster } from "../src";
  import Gallery from "./Gallery.svelte";

  $effect(() => followSystemTheme());
</script>

<header>
  <h1>Komponenten</h1>
  <ThemeSwitch />
</header>
<main>
  <div class="panel" data-theme="light"><Gallery /></div>
  <div class="panel" data-theme="dark"><Gallery /></div>
</main>
<Toaster />

<style>
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-6);
    border-bottom: 1px solid var(--color-border);
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
  }
  main {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(420px, 1fr));
  }
  .panel {
    padding: 0 var(--space-6) var(--space-6);
    background: var(--color-bg);
    color: var(--color-text);
  }
</style>
