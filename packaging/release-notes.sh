#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Print the notes of release <version>: the version's section of CHANGELOG.md under "What's new",
# which the app shows when it offers the release, then packaging/release-notes.md (downloads,
# licence, verification). The release workflow drafts the release with them.
#
#   packaging/release-notes.sh 0.3.0 > notes.md
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:?usage: release-notes.sh <version>}"

# The lines after "## [<version>]" up to the next "## [" heading, without the link list at the end.
section="$(awk -v v="$VERSION" '
  /^## \[/ { if (found) exit; if (index($0, "## [" v "]") == 1) { found = 1; next } }
  /^\[[^]]+\]: / { next }
  found { print }
' "$ROOT/CHANGELOG.md")"

if [ -n "$(printf '%s' "$section" | tr -d '[:space:]')" ]; then
  printf "## What's new in %s\n%s\n\n" "$VERSION" "$section"
else
  # A test version (0.3.0-rc.1) has no section of its own: the notes say so.
  printf "## What's new in %s\n\nSee CHANGELOG.md.\n\n" "$VERSION"
fi
cat "$ROOT/packaging/release-notes.md"
