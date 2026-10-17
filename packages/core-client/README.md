# packages/core-client

TS-Interface `CoreClient` + Implementierungen. Die UI hängt ausschließlich von diesem Interface ab
(`development/interfaces.md` §7).

- `@tasks/core-client` – Interface, DTO-Typen (generiert aus Rust, `src/generated/`), `CoreClientError`,
  `FakeCoreClient` (In-Memory; Browser-Entwicklung und Komponententests)
- `@tasks/core-client/tauri` – `TauriCoreClient` (`invoke`/`listen`); einzige Datei mit `@tauri-apps/*`

DTO-Typen neu erzeugen: `just types`.
