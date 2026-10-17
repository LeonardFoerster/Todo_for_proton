# apps/desktop – Tauri-2-Shell (Linux)

- `src/` – Vite + Svelte 5; mountet die App aus `packages/ui` mit `TauriCoreClient`
- `src-tauri/` – Rust Composition Root (Crate `todo-desktop`): dünne `async` Commands → `todo_core::api`

Systempakete (Arch): `webkit2gtk-4.1 base-devel curl wget file openssl librsvg` (laut Tauri-Doku).

```sh
just desktop-dev     # Dev-Fenster mit Hot-Reload (Vite auf Port 1420)
just desktop-build   # Release-Binary: target/release/todo-desktop (ohne Pakete)
```

NVIDIA + Wayland: Bei weißem Fenster/Flackern `WEBKIT_DISABLE_DMABUF_RENDERER=1` setzen (Workaround, nicht eingebaut).
