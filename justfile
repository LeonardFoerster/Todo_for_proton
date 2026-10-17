# Entwickler-Kommandos. Übersicht: `just --list`

# Standard-Prüfung vor jedem Commit: Format, Lints, Tests
check: fmt-check clippy test doc wasm

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

# API-Doku bauen, Warnungen (z. B. kaputte Links) = Fehler
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Kompiliert die Web-Composition-Root und todo-core inkl. Tests (?Send-Ports) für wasm32.
# todo-core allein braucht dort `uuid/js` als Zufallsquelle (liefert sonst todo-wasm).
wasm:
    cargo check -p todo-wasm --target wasm32-unknown-unknown
    cargo check -p todo-core --target wasm32-unknown-unknown --all-targets --features uuid/js,test-support
