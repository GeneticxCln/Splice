#!/usr/bin/env bash
# Builds every release artifact into OUTDIR:
#   splice-vX-linux-x86_64.tar.gz   binary tarball
#   Splice-X.tar.gz                 source tarball (what the PKGBUILD builds)
#   splice-editor_X-1_amd64.deb
#   splice-editor-X-1.x86_64.rpm
#   Splice-X-x86_64.AppImage        only when APPIMAGETOOL is set
#   PKGBUILD, splice-editor.SRCINFO Arch Linux, checksum filled in (rename the
#                                   second to .SRCINFO for the AUR)
#   SHA256SUMS
#
#   packaging/build-packages.sh 1.4.0 dist
#
# Needs cargo, cargo-deb, cargo-generate-rpm and git. Run it on the commit that
# is being released: the source tarball is `git archive HEAD`.
set -euo pipefail

version="${1:?usage: build-packages.sh VERSION OUTDIR}"
out="${2:?missing OUTDIR}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

crate_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
if [ "$version" != "$crate_version" ]; then
    echo "version $version does not match Cargo.toml ($crate_version)" >&2
    exit 1
fi

mkdir -p "$out"
out="$(cd "$out" && pwd)"

echo "== build =="
cargo build --release --locked

echo "== binary tarball =="
name="splice-v$version-linux-x86_64"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name"
cp target/release/splice README.md LICENSE CHANGELOG.md "$stage/$name/"
cp -r packaging "$stage/$name/"
tar -C "$stage" -czf "$out/$name.tar.gz" "$name"

echo "== source tarball =="
git archive --format=tar.gz --prefix="Splice-$version/" -o "$out/Splice-$version.tar.gz" HEAD

echo "== deb =="
cargo deb --no-build --no-strip --output "$out/"

echo "== rpm =="
cargo generate-rpm --output "$out/"

if [ -n "${APPIMAGETOOL:-}" ]; then
    echo "== AppImage =="
    "$root/packaging/build-appimage.sh" target/release/splice "$version" "$out"
    rm -rf "$out/Splice.AppDir"
else
    echo "== AppImage skipped (APPIMAGETOOL not set) =="
fi

echo "== PKGBUILD =="
source_sum="$(sha256sum "$out/Splice-$version.tar.gz" | cut -d' ' -f1)"
sed -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^sha256sums=.*/sha256sums=('$source_sum')/" \
    packaging/arch/PKGBUILD > "$out/PKGBUILD"
packaging/arch/mksrcinfo.sh "$out/PKGBUILD" > "$out/splice-editor.SRCINFO"

echo "== checksums =="
(cd "$out" && sha256sum -- $(ls -1 | grep -v '^SHA256SUMS$') > SHA256SUMS)
ls -la "$out"
