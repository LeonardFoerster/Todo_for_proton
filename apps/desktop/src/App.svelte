<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import "@tasks/ui/tokens.css";

  onMount(() => {
    requestAnimationFrame(() => requestAnimationFrame(() => invoke("app_ready")));
  });
</script>

<main>
  <h1>Tasks</h1>
</main>
