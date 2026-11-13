#!/usr/bin/env bash
# Assembles an AppDir and, when APPIMAGETOOL is set, packs it into an AppImage.
#   packaging/build-appimage.sh BINARY VERSION OUTDIR
#
# The AppImage bundles only Splice itself. It relies on the host for ffmpeg
# (with libx264 and aac) and for the usual desktop libraries (ALSA, OpenGL,
# Wayland or X11), exactly like the other packages.
set -euo pipefail

binary="${1:?usage: build-appimage.sh BINARY VERSION OUTDIR}"
version="${2:?missing VERSION}"
out="${3:?missing OUTDIR}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

appdir="$out/Splice.AppDir"
rm -rf "$appdir"
mkdir -p "$appdir/usr/bin" \
         "$appdir/usr/share/applications" \
         "$appdir/usr/share/icons/hicolor/scalable/apps" \
         "$appdir/usr/share/metainfo"

install -m755 "$binary" "$appdir/usr/bin/splice"
install -m644 "$here/splice.desktop" "$appdir/usr/share/applications/splice.desktop"
install -m644 "$here/splice.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/splice.svg"
# appimagetool looks for these two at the AppDir root.
install -m644 "$here/splice.desktop" "$appdir/splice.desktop"
install -m644 "$here/splice.svg" "$appdir/splice.svg"
ln -sf splice.svg "$appdir/.DirIcon"

cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/splice" "$@"
APPRUN
chmod +x "$appdir/AppRun"

if [ -z "${APPIMAGETOOL:-}" ]; then
    echo "AppDir ready at $appdir (set APPIMAGETOOL to build the AppImage)"
    exit 0
fi

target="$out/Splice-$version-x86_64.AppImage"
ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run "$appdir" "$target"
chmod +x "$target"
echo "built $target"
