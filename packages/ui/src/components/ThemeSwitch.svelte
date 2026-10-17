<script lang="ts">
  import { Monitor, Moon, Sun } from "../icons";
  import { readPreference, setThemePreference, type ThemePreference } from "../theme/theme";

  let preference = $state<ThemePreference>(readPreference());

  const options = [
    { value: "system", label: "System", icon: Monitor },
    { value: "light", label: "Hell", icon: Sun },
    { value: "dark", label: "Dunkel", icon: Moon },
  ] as const;

  function choose(value: ThemePreference) {
    preference = value;
    setThemePreference(value);
  }
</script>

<div class="switch" role="radiogroup" aria-label="Darstellung">
  {#each options as option (option.value)}
    <button
      type="button"
      role="radio"
      aria-checked={preference === option.value}
      onclick={() => choose(option.value)}
    >
      <option.icon size={14} strokeWidth={1.75} aria-hidden="true" />
      {option.label}
    </button>
  {/each}
</div>

<style>
  .switch {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-md);
    background: var(--color-bg-hover);
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }
  button:hover {
    color: var(--color-text);
  }
  button[aria-checked="true"] {
    background: var(--color-bg);
    color: var(--color-text);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
  }
</style>
