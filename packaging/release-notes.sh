#!/usr/bin/env bash
# Prints the CHANGELOG.md section for one version (without the heading).
#   packaging/release-notes.sh 1.4.0
set -euo pipefail
version="${1:?usage: release-notes.sh VERSION}"
changelog="$(dirname "${BASH_SOURCE[0]}")/../CHANGELOG.md"
awk -v v="$version" '
  $0 == "## " v { found = 1; next }
  found && /^## / { exit }
  found { print }
' "$changelog" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}'
