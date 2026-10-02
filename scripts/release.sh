#!/usr/bin/env bash
# Cuts a Vitela release: tags the current origin/main and pushes the tag,
# which is what starts every platform's release workflow.
#
#   scripts/release.sh 0.2.0-beta.1      (asks before pushing)
#   scripts/release.sh v0.2.0 --yes
#
# It tags origin/main as fetched right now, never the local checkout, so
# unpushed commits or another checked-out branch cannot end up in a release.
# The version must be newer than every release tag already on origin:
# packages only upgrade forwards, so a release can never go backwards.
set -euo pipefail

readonly VERSION_SCRIPT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/release-version.sh"

fail() { printf 'release: %s\n' "$*" >&2; exit 1; }

version_arg='' assume_yes=0
for arg in "$@"; do
    case "$arg" in
        --yes|-y) assume_yes=1 ;;
        -*) fail "unknown option: $arg" ;;
        *) [ -z "$version_arg" ] || fail 'more than one version given'; version_arg="$arg" ;;
    esac
done
[ -n "$version_arg" ] || fail 'usage: release.sh <version> [--yes]   e.g. release.sh 0.2.0-beta.1'

readonly TAG="v${version_arg#v}"
bash "$VERSION_SCRIPT" "$TAG" semver >/dev/null || exit 1

git fetch --quiet --tags origin main || fail 'could not fetch origin'
target="$(git rev-parse --verify --quiet 'origin/main^{commit}')" || fail 'origin/main not found'

if git rev-parse --verify --quiet "refs/tags/$TAG" >/dev/null; then
    fail "tag $TAG already exists"
fi

latest=''
while IFS= read -r tag; do
    bash "$VERSION_SCRIPT" "$tag" semver >/dev/null 2>&1 || continue
    if [ -z "$latest" ] || bash "$VERSION_SCRIPT" "$tag" newer-than "$latest"; then
        latest="$tag"
    fi
done < <(git ls-remote --tags --refs origin 'v*' | sed 's|.*refs/tags/||')

if [ -n "$latest" ] && ! bash "$VERSION_SCRIPT" "$TAG" newer-than "$latest"; then
    fail "$TAG is not newer than the latest release $latest"
fi

printf 'Release %s\n' "$TAG"
printf '  commit:   %s\n' "$(git log -1 --format='%h %s' "$target")"
printf '  previous: %s\n' "${latest:-none}"
if [ "$assume_yes" -ne 1 ]; then
    read -r -p 'Tag origin/main and push? [y/N] ' answer || answer=''
    case "$answer" in y|Y|yes|YES) ;; *) fail 'aborted, nothing was tagged' ;; esac
fi

git tag -a "$TAG" -m "Vitela $TAG" "$target"
if ! git push --quiet origin "refs/tags/$TAG"; then
    git tag -d "$TAG" >/dev/null
    fail "push failed; the local tag $TAG was removed"
fi
printf 'Pushed %s. The release workflows are running now.\n' "$TAG"
