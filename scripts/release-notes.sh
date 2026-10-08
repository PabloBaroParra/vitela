#!/usr/bin/env bash
# The Google Play "what's new" text of a release tag, written to stdout:
#
#   release-notes.sh v0.2.0-beta.2
#   • Read saved comments
#   • Accented text in form fields
#
# One line per feat/fix commit since the previous tag (or since the start of
# history, for the first tag), oldest first. Only changes an Android user can
# see are listed: unscoped commits, scope android, and the shared behaviour
# scopes (form, annotate). Other shells, core plumbing, CI and docs are left
# out. With nothing left, a generic line is printed, because Play shows the
# previous release's notes when a release has none.
#
# Play refuses more than 500 characters per language, so the list stops at
# the last whole line that fits.
set -euo pipefail

fail() { printf 'release-notes: %s\n' "$*" >&2; exit 1; }

readonly LIMIT=500
readonly SCOPES='android|form|annotate'

[ "$#" -eq 1 ] || fail 'usage: release-notes.sh <tag>'
readonly TAG="$1"
git rev-parse --verify -q "refs/tags/$TAG^{commit}" >/dev/null || fail "no such tag: $TAG"

if previous="$(git describe --tags --abbrev=0 --match 'v*' "$TAG^" 2>/dev/null)"; then
    range="$previous..$TAG"
else
    range="$TAG"
fi

notes=''
while IFS= read -r subject; do
    [[ "$subject" =~ ^(feat|fix)(\(($SCOPES)\))?!?:\ +(.+)$ ]] || continue
    text="${BASH_REMATCH[4]}"
    text="$(sed -E 's/ \(#[0-9]+\)$//' <<< "$text")"
    line="• ${text^}"
    candidate="${notes:+$notes$'\n'}$line"
    (( ${#candidate} <= LIMIT )) || break
    notes="$candidate"
done < <(git log --reverse --no-merges --format=%s "$range")

printf '%s\n' "${notes:-Bug fixes and improvements.}"
