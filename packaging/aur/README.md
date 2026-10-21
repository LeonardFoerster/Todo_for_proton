# AUR-Paket `tasks-app-git` (N11-01)

- `PKGBUILD` baut den aktuellen Stand aus Git (cargo mit der Toolchain aus `rust-toolchain.toml` über rustup,
  pnpm für Vite und die Tauri CLI) und installiert `/usr/bin/tasks-app`, den Starter
  `dev.tasks.app.desktop` und die Icons.
- Lokal prüfen (ohne Installation): `makepkg -f` in diesem Ordner; `.SRCINFO` mit
  `makepkg --printsrcinfo > .SRCINFO` neu erzeugen.
- Vor dem Veröffentlichen: Lizenz festlegen (README „Lizenz“) und in `license=()` eintragen.
- Veröffentlichen im AUR macht der User (eigenes AUR-Repo `tasks-app-git` mit `PKGBUILD`, `.SRCINFO` und
  `dev.tasks.app.desktop` – die Desktop-Datei liegt auch im Hauptrepo, der Build nimmt die von dort).
