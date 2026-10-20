#!/usr/bin/env bash
# Live-Abnahme P6-03 … P6-05 gegen das Proton-TESTKONTO. Vom User im eigenen Terminal starten:
#   bash development/live-p6.sh
# Drei Browser-Bestätigungen (je ein Login für P6-03, P6-04 und die CLI). Kein Passwort im Terminal.
# Volle Ausgabe im Terminal; bereinigte Zusammenfassung (ohne Login-URLs/Kontonamen) in
# development/live-p6-summary.txt – diese Datei bitte in die Session kopieren.
set -uo pipefail
cd "$(dirname "$0")/.."
SUMMARY=development/live-p6-summary.txt
: >"$SUMMARY"

redact() {
  sed -E \
    -e 's#https://account\.proton\.me[^ ]*#<login-url>#g' \
    -e 's#(angemeldet als|Angemeldet als) [^ ]+#\1 <konto>#g' \
    -e 's#[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[a-z]+#<mail>#g'
}

section() { echo; echo "===== $* =====" | tee -a "$SUMMARY"; }
log() { redact | tee -a "$SUMMARY"; }

section "Build (release)"
cargo build --release -p todo-cli 2>&1 | tail -1 | log
CLI=target/release/todo-cli

export PROTON_LIVE=1
section "P6-03 · ProtonAuthProvider live"
cargo test --release -p proton-adapter --test live_auth -- --ignored --nocapture 2>&1 \
  | grep -E '^\[|ERGEBNIS|panicked|test result|This is a third' | log

section "P6-04 · RemoteStore-Contract live"
cargo test --release -p proton-adapter --lib drive::live -- --ignored --nocapture 2>&1 \
  | grep -E '^\[|ERGEBNIS|panicked|test result|fehlgeschlagen' | log

section "P6-05 · Zwei Geräte über Proton (CLI)"
TMP=$(mktemp -d)
A="$CLI --db $TMP/a.sqlite3"
B="$CLI --db $TMP/b.sqlite3"
step() { echo "\$ todo-cli $*" | sed -E "s#--db $TMP/##" | log; }
run() { local who=$1; shift; step "[$who] $*"; if [ "$who" = A ]; then $A "$@"; else $B "$@"; fi 2>&1 | log; }

step "login"; $CLI login 2>&1 | log
run A add Milch kaufen
run A list-add Arbeit
run A add Bericht schreiben -l Arbeit
run A sync --remote proton
run A sync --remote proton
run B sync --remote proton
run B ls
run B ls Arbeit
run B lists
run B add Von Gerät B
run B sync --remote proton
run A sync --remote proton
run A ls
run A sync --remote proton

section "Sichtprüfung Weboberfläche"
echo "Bitte jetzt in der Proton-Drive-Weboberfläche den Ordner „Tasks“ ansehen:"
echo "  sinnvolle Namen (ops/<geräte-id>/…, keys/, devices/, manifest.json)? Segment-Inhalte unlesbar?"
read -r -p "Kurzer Befund (eine Zeile): " FINDING
echo "Befund: $FINDING" | log

section "Abmelden"
$CLI logout 2>&1 | log
rm -rf "$TMP"
echo
echo "Fertig. Zusammenfassung: $SUMMARY (bitte in die Session kopieren)."
echo "Der Ordner „Tasks“ im Testkonto bleibt bestehen; bei Bedarf in der Weboberfläche löschen."
