#!/usr/bin/env bash
# Prints .SRCINFO for a PKGBUILD without needing makepkg (so CI on Ubuntu can
# produce it). Only the fields this package uses are emitted.
#   packaging/arch/mksrcinfo.sh packaging/arch/PKGBUILD > .SRCINFO
set -euo pipefail
file="${1:?usage: mksrcinfo.sh PKGBUILD}"
# shellcheck disable=SC1090
source "$file"

echo "pkgbase = $pkgname"
echo "	pkgdesc = $pkgdesc"
echo "	pkgver = $pkgver"
echo "	pkgrel = $pkgrel"
echo "	url = $url"
for a in "${arch[@]}"; do echo "	arch = $a"; done
for l in "${license[@]}"; do echo "	license = $l"; done
for d in "${makedepends[@]}"; do echo "	makedepends = $d"; done
for d in "${depends[@]}"; do echo "	depends = $d"; done
for d in "${optdepends[@]}"; do echo "	optdepends = $d"; done
for s in "${source[@]}"; do echo "	source = $s"; done
for c in "${sha256sums[@]}"; do echo "	sha256sums = $c"; done
echo
echo "pkgname = $pkgname"
