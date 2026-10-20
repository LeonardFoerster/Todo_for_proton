#!/usr/bin/env bash
# Nur der P6-04-Live-Contract-Test (nach Fix e35862a). Ein Browser-Login.
cd "$(dirname "$0")/.."
OUT=development/live-p6-04-summary.txt
PROTON_LIVE=1 cargo test --release -p proton-adapter --lib drive::live -- --ignored --nocapture 2>&1 \
  | grep -E '^\[|ERGEBNIS|panicked|test result|fehlgeschlagen' \
  | sed -E 's#https://account\.proton\.me[^ ]*#<login-url>#g' | tee "$OUT.tmp"
mv "$OUT.tmp" "$OUT"
echo "Fertig: $OUT"
