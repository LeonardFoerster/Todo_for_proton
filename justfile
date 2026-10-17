# Entwickler-Kommandos. Übersicht: `just --list`

# Standard-Prüfung vor jedem Commit: Format, Lints, Tests
check: fmt-check clippy test

# Alle Rust-Tests im Workspace
test:
    cargo test --workspace --all-targets

# Code formatieren
fmt:
    cargo fmt --all

# Formatierung nur prüfen (ändert nichts)
fmt-check:
    cargo fmt --all --check

# Clippy mit Warnungen als Fehler
clippy:
    cargo clippy --workspace --all-targets -- -D warnings
