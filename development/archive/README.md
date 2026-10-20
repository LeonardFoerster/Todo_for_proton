# Archiv

Abgelegte Wegwerf-Artefakte (P8-04, 2026-10-18). Aus `spikes/` blieb nur `proton-research.md` (wird noch zitiert).

- `proton-probe-src.tar.gz` – Quellen der Proton-Probe aus P2 (Login per Session-Fork, Refresh/Logout, Drive-Probe;
  Erkenntnisse in `spikes/proton-research.md`). Entpacken mit `tar xzf … -C spikes/`, bauen mit `cargo build` im Ordner.
- Referenz-Klone, nur gelesen, nie kopiert (GPL-3.0/Marke, AGENTS.md §7) – bei Bedarf neu klonen:
  - https://github.com/rclone/go-proton-api.git @ 4e6ddcf
  - https://github.com/rclone/Proton-API-Bridge.git @ 9d772d0
  - https://github.com/ProtonDriveApps/sdk.git @ 8ffae00
- `roadmap-prototype.md` – vollständige Roadmap bis zum Prototyp (P0–P8 mit Notizen, FIX-01…07), Stand 2026-10-18.
  Alte Aufgaben-IDs wie „P7-08" oder „P5-11" in anderen Dokumenten beziehen sich auf **diese** Datei; `roadmap.md`
  wurde danach geleert und neu nummeriert.
- `roadmap-interim.md` – abgeschlossene Zwischenphasen P0–P2 aus `roadmap.md` (Mehrfachauswahl, Upload ≤ 5 s,
  Listen & Kanban), verschoben 2026-10-18. IDs überschneiden sich mit `roadmap-prototype.md` (Hinweis im Dateikopf).
- `live-p6/` – Live-Skripte der Prototyp-Phase P6 (`live-p6.sh`, `live-p6-04.sh`) und ihre Zusammenfassung. Die Skripte
  erwarten, vom Repo-Wurzelverzeichnis aus mit dem alten Pfad `development/` gestartet zu werden – bei Bedarf vorher
  zurückkopieren. Für neue Live-Läufe gilt `prototype-acceptance.md` Teil B (N0-01).
