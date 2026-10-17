<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  // Erster Frame steht → Shell meldet Startzeit (tools/measure-desktop.sh).
  onMount(() => {
    requestAnimationFrame(() => requestAnimationFrame(() => invoke("app_ready")));
  });
</script>

<main>
  <h1>Tasks</h1>
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    padding: 2rem;
  }
</style>
