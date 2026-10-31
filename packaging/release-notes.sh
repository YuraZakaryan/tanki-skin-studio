#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Print the notes of release <version>: the version's section of CHANGELOG.md under "What's new",
# then the one of CHANGELOG.ru.md under "Что нового" (the app shows the section of its interface
# language when it offers the release, English when there is none), then
# packaging/release-notes.md (downloads, licence, verification). The release workflow drafts the
# release with them. The English section comes first: copies up to 0.3.0 read only the first
# "What's new" section and stop at the next heading, so the Russian one never mixes into theirs.
#
#   packaging/release-notes.sh 0.3.0 > notes.md
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:?usage: release-notes.sh <version>}"

# The lines of <file> after "## [<version>]" up to the next "## [" heading, without the link list.
section() {
  awk -v v="$VERSION" '
    /^## \[/ { if (found) exit; if (index($0, "## [" v "]") == 1) { found = 1; next } }
    /^\[[^]]+\]: / { next }
    found { print }
  ' "$1"
}
has_text() { [ -n "$(printf '%s' "$1" | tr -d '[:space:]')" ]; }

english="$(section "$ROOT/CHANGELOG.md")"
if has_text "$english"; then
  printf "## What's new in %s\n%s\n\n" "$VERSION" "$english"
else
  # A test version (0.3.0-rc.1) has no section of its own: the notes say so.
  printf "## What's new in %s\n\nSee CHANGELOG.md.\n\n" "$VERSION"
fi

russian=""
[ -f "$ROOT/CHANGELOG.ru.md" ] && russian="$(section "$ROOT/CHANGELOG.ru.md")"
if has_text "$russian"; then
  printf "## Что нового в %s\n%s\n\n" "$VERSION" "$russian"
elif has_text "$english"; then
  # Not fatal: the app shows the English notes instead. In a workflow run this is a visible warning.
  echo "::warning::CHANGELOG.ru.md has no section for $VERSION; Russian users see the English notes" >&2
fi
cat "$ROOT/packaging/release-notes.md"
