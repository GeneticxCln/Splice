#!/usr/bin/env bash
# Installs Splice for the current user (default) or system-wide with PREFIX.
#   packaging/install.sh                  -> ~/.local
#   PREFIX=/usr/local sudo packaging/install.sh
# Expects a built binary at target/release/splice or ./splice (release tarball).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"

BIN=""
for candidate in "$HERE/../splice" "$HERE/../target/release/splice"; do
    if [ -x "$candidate" ]; then BIN="$candidate"; break; fi
done
[ -n "$BIN" ] || { echo "install: build first (cargo build --release)" >&2; exit 1; }

install -Dm755 "$BIN" "$PREFIX/bin/splice"
install -Dm644 "$HERE/splice.desktop" "$PREFIX/share/applications/splice.desktop"
install -Dm644 "$HERE/splice.svg" "$PREFIX/share/icons/hicolor/scalable/apps/splice.svg"
install -Dm644 "$HERE/splice-project.xml" "$PREFIX/share/mime/packages/splice-project.xml"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$PREFIX/share/applications" || true
command -v update-mime-database >/dev/null 2>&1 && update-mime-database "$PREFIX/share/mime" || true
echo "Installed Splice to $PREFIX"
