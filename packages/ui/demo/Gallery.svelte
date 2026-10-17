<script lang="ts">
  import { Button, Checkbox, IconButton, Textarea, TextInput, toasts, VirtualList } from "../src";
  import { Ellipsis, PanelLeft, Plus, RefreshCw, Trash } from "../src/icons";

  let a = $state(false);
  let b = $state(true);
  let text = $state("");
  const rows = Array.from({ length: 1000 }, (_, i) => ({ id: String(i), title: `Task ${i + 1}`, done: i % 5 === 0 }));
</script>

<section>
  <h2>Buttons</h2>
  <div class="row">
    <Button variant="primary" icon={Plus}>Neue Task</Button>
    <Button>Abbrechen</Button>
    <Button variant="ghost">Mehr anzeigen</Button>
    <Button variant="danger" icon={Trash}>Liste löschen</Button>
    <Button variant="primary" disabled>Deaktiviert</Button>
  </div>
  <div class="row">
    <Button variant="primary" size="lg" icon={Plus}>Neue Task</Button>
    <IconButton label="Seitenleiste einklappen" icon={PanelLeft} />
    <IconButton label="Synchronisieren" icon={RefreshCw} shortcut="Strg+R" />
    <IconButton label="Weitere Aktionen" icon={Ellipsis} size={28} />
    <IconButton label="Aktiv" icon={PanelLeft} pressed />
  </div>
</section>

<section>
  <h2>Checkbox</h2>
  <div class="row">
    <Checkbox checked={a} label="Offen" onchange={(v) => (a = v)} />
    <Checkbox checked={b} label="Erledigt" onchange={(v) => (b = v)} />
    <Checkbox checked={false} label="Hoch" tone="high" />
    <Checkbox checked={false} label="Mittel" tone="medium" />
    <Checkbox checked={false} label="Niedrig" tone="low" />
    <Checkbox checked={false} label="Deaktiviert" disabled />
  </div>
</section>

<section>
  <h2>Eingaben</h2>
  <div class="stack">
    <TextInput bind:value={text} placeholder="Task hinzufügen …" aria-label="Neue Task" />
    <TextInput value="Ungültig" invalid aria-label="Ungültig" />
    <TextInput value="Leiser Titel" quiet aria-label="Titel" />
    <Textarea placeholder="Notizen" aria-label="Notizen" rows={3} />
  </div>
</section>

<section>
  <h2>Toast</h2>
  <div class="row">
    <Button
      onclick={() =>
        toasts.show({ message: "Task erledigt", actionLabel: "Rückgängig", onAction: () => toasts.show({ message: "Rückgängig gemacht" }) })}
    >
      Toast zeigen
    </Button>
    <div class="toast-preview">
      <span>„Milch kaufen“ gelöscht</span><b>Rückgängig</b>
    </div>
  </div>
</section>

<section>
  <h2>VirtualList (1 000 Zeilen)</h2>
  <div class="list">
    <VirtualList items={rows} key={(r) => r.id} label="Demo-Liste">
      {#snippet row(item)}
        <div class="vrow" class:done={item.done} role="listitem">
          <Checkbox checked={item.done} label={item.title} />
          <span>{item.title}</span>
        </div>
      {/snippet}
    </VirtualList>
  </div>
</section>

<style>
  section {
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--color-border);
  }
  h2 {
    margin: 0 0 var(--space-3);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }
  .stack {
    display: grid;
    gap: var(--space-2);
    max-width: 360px;
  }
  .toast-preview {
    display: flex;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-bg-inverse);
    color: var(--color-text-inverse);
  }
  .toast-preview b {
    color: var(--color-accent-inverse);
  }
  .list {
    height: 220px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .vrow {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 36px;
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--color-border);
  }
  .done span {
    color: var(--color-text-muted);
    text-decoration: line-through;
  }
</style>
