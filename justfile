# Entwickler-Kommandos. Übersicht: `just --list`

# Standard-Prüfung vor jedem Commit: Format, Lints, Tests
check: fmt-check clippy test

# Alle Rust-Tests im Workspace (`--all-targets` enthält keine Doctests, daher zweiter Lauf)
test:
    cargo test --workspace --all-targets
    cargo test --workspace --doc

# Code formatieren
fmt:
    cargo fmt --all

# Formatierung nur prüfen (ändert nichts)
fmt-check:
    cargo fmt --all --check

# Clippy mit Warnungen als Fehler
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings
