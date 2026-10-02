#!/usr/bin/env bash
# Shell tests for the release tooling every platform shares:
#   scripts/release-version.sh  tag grammar + per-platform version formats
#   scripts/release.sh          the one command a maintainer runs to release
#
# release.sh cases run against a throwaway clone of a throwaway bare
# "origin", so nothing here can push anywhere real.
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly VERSION_SCRIPT="$REPO_ROOT/scripts/release-version.sh"
readonly RELEASE_SCRIPT="$REPO_ROOT/scripts/release.sh"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
assert_file() { [ -f "$1" ] || fail "expected file: $1"; }

version() { bash "$VERSION_SCRIPT" "$@"; }
skipped=0

test_tag_maps_to_each_format() {
    [ "$(version v0.2.0-beta.1 semver)" = '0.2.0-beta.1' ] || fail 'semver of a beta tag'
    [ "$(version v0.2.0 semver)" = '0.2.0' ] || fail 'semver of a final tag'
    [ "$(version v0.2.0-beta.1 debian)" = '0.2.0~beta.1' ] || fail 'debian of a beta tag'
    [ "$(version v0.2.0-rc.2 debian)" = '0.2.0~rc.2' ] || fail 'debian of an rc tag'
    [ "$(version v0.2.0-alpha.3 debian)" = '0.2.0~alpha.3' ] || fail 'debian of an alpha tag'
    [ "$(version v0.2.0 debian)" = '0.2.0' ] || fail 'debian of a final tag'
    [ "$(version v10.20.30 debian)" = '10.20.30' ] || fail 'multi-digit components'
}

test_debian_versions_sort_like_the_releases() {
    command -v dpkg >/dev/null 2>&1 || { printf 'skip: dpkg not available\n'; skipped=$((skipped + 1)); return; }
    local ordered=(v0.1.0 v0.2.0-alpha.1 v0.2.0-alpha.2 v0.2.0-beta.1 v0.2.0-beta.10 v0.2.0-rc.1 v0.2.0 v0.2.1 v0.10.0)
    local i
    for ((i = 1; i < ${#ordered[@]}; i++)); do
        dpkg --compare-versions "$(version "${ordered[i-1]}" debian)" lt "$(version "${ordered[i]}" debian)" \
            || fail "debian order: ${ordered[i-1]} must sort before ${ordered[i]}"
    done
}

test_malformed_tags_and_formats_fail() {
    local tag
    for tag in 0.2.0 v0.2 v0.2.0- v0.2.0-beta v0.2.0-beta.0 v0.2.0-beta.01 v0.2.0-preview.1 v0.2.0-beta_1 \
        'v0.2.0-beta 1' v0.2.0+build.1 v01.0.0 V0.2.0 ''; do
        if version "$tag" debian >/dev/null 2>&1; then fail "accepted malformed tag: '$tag'"; fi
    done
    if version v0.2.0 msi >/dev/null 2>&1; then fail 'accepted an unknown format'; fi
    if version v0.2.0 >/dev/null 2>&1; then fail 'accepted a missing format'; fi
}

test_versions_compare_in_release_order() {
    local ordered=(v0.1.0 v0.2.0-alpha.1 v0.2.0-alpha.2 v0.2.0-beta.1 v0.2.0-beta.10 v0.2.0-rc.1 v0.2.0 v0.2.1 v0.10.0)
    local i
    for ((i = 1; i < ${#ordered[@]}; i++)); do
        version "${ordered[i]}" newer-than "${ordered[i-1]}" >/dev/null \
            || fail "${ordered[i]} must be newer than ${ordered[i-1]}"
        if version "${ordered[i-1]}" newer-than "${ordered[i]}" >/dev/null 2>&1; then
            fail "${ordered[i-1]} must not be newer than ${ordered[i]}"
        fi
    done
    if version v0.2.0 newer-than v0.2.0 >/dev/null 2>&1; then fail 'a version is not newer than itself'; fi
}

# --- release.sh -------------------------------------------------------------

sandbox=''
cleanup() { [ -z "$sandbox" ] || rm -rf -- "$sandbox"; }
trap cleanup EXIT

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@invalid

make_sandbox() {
    cleanup
    sandbox="$(mktemp -d "${TMPDIR:-/tmp}/vitela release.XXXXXX")"
    git init -q --bare -b main "$sandbox/origin.git"
    git clone -q "$sandbox/origin.git" "$sandbox/clone" 2>/dev/null
    git -C "$sandbox/clone" commit -q --allow-empty -m 'first'
    git -C "$sandbox/clone" push -q origin main
}

release() { (cd "$sandbox/clone" && bash "$RELEASE_SCRIPT" "$@"); }
remote_tag_commit() { git -C "$sandbox/origin.git" rev-parse --verify -q "refs/tags/$1^{commit}"; }

test_release_tags_origin_main_and_pushes() {
    make_sandbox
    release 0.2.0-beta.1 --yes >/dev/null
    [ "$(remote_tag_commit v0.2.0-beta.1)" = "$(git -C "$sandbox/origin.git" rev-parse main)" ] \
        || fail 'tag does not point at origin/main'
    [ "$(git -C "$sandbox/origin.git" cat-file -t v0.2.0-beta.1)" = tag ] || fail 'tag is not annotated'
}

test_release_ignores_local_commits_and_branches() {
    make_sandbox
    git -C "$sandbox/clone" checkout -q -b feature
    git -C "$sandbox/clone" commit -q --allow-empty -m 'unpushed work'
    release v0.2.0 --yes >/dev/null
    [ "$(remote_tag_commit v0.2.0)" = "$(git -C "$sandbox/origin.git" rev-parse main)" ] \
        || fail 'tagged the local checkout instead of origin/main'
}

test_release_refuses_existing_or_older_versions() {
    make_sandbox
    release 0.2.0 --yes >/dev/null
    if release 0.2.0 --yes >/dev/null 2>&1; then fail 'released the same version twice'; fi
    if release 0.2.0-rc.1 --yes >/dev/null 2>&1; then fail 'released a prerelease of an already final version'; fi
    if release 0.1.9 --yes >/dev/null 2>&1; then fail 'released an older version'; fi
    release 0.2.1-beta.1 --yes >/dev/null || fail 'refused a newer version'
}

test_release_refuses_malformed_versions() {
    make_sandbox
    if release 0.2 --yes >/dev/null 2>&1; then fail 'released a malformed version'; fi
    if release 0.2.0-preview.1 --yes >/dev/null 2>&1; then fail 'released an unknown prerelease word'; fi
    [ -z "$(git -C "$sandbox/origin.git" tag)" ] || fail 'a refused release still pushed a tag'
}

test_release_asks_before_pushing() {
    make_sandbox
    if printf 'n\n' | release 0.2.0 >/dev/null 2>&1; then fail 'treated "n" as consent'; fi
    [ -z "$(git -C "$sandbox/origin.git" tag)" ] || fail 'pushed without consent'
    [ -z "$(git -C "$sandbox/clone" tag)" ] || fail 'left a local tag behind after declining'
    printf 'y\n' | release 0.2.0 >/dev/null || fail 'refused after "y"'
    remote_tag_commit v0.2.0 >/dev/null || fail 'did not push after consent'
}

test_required_assets_exist() {
    assert_file "$VERSION_SCRIPT"
    assert_file "$RELEASE_SCRIPT"
}

test_tag_maps_to_each_format
test_debian_versions_sort_like_the_releases
test_malformed_tags_and_formats_fail
test_versions_compare_in_release_order
test_release_tags_origin_main_and_pushes
test_release_ignores_local_commits_and_branches
test_release_refuses_existing_or_older_versions
test_release_refuses_malformed_versions
test_release_asks_before_pushing
test_required_assets_exist
printf 'release tooling shell tests: %d passed, %d skipped\n' $((10 - skipped)) "$skipped"
