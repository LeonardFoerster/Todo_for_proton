# packages/ui

Svelte-5-Komponenten und Design-Tokens (Proton-nah, eigene Umsetzung). Richtlinien: `development/ui-guidelines.md`.
Kein direkter Zugriff auf Tauri, WASM oder das Sync-Backend – nur über `CoreClient` (`@tasks/core-client`).

- `@tasks/ui` – Komponenten (`Button`, `IconButton`, `Checkbox`, `TextInput`, `Textarea`, `Toaster`/`toasts`,
  `VirtualList`, `ThemeSwitch`) und Theme-Funktionen
- `@tasks/ui/tokens.css` – Tokens hell/dunkel, Grundstile, Tooltips (`data-tooltip`)
- `@tasks/ui/vite` – Vite-Plugin `themeInit()`: setzt `data-theme` vor dem ersten Paint
- Icons: nur über `src/icons.ts` (Lucide, einzeln importiert)

Komponenten sichten (beide Themes nebeneinander): `pnpm --filter @tasks/ui dev` → http://localhost:5180.
Kontraste der Token-Paare prüft `src/tokens/tokens.test.ts`.
