#!/usr/bin/env bash
# Shell tests for the release tooling every platform shares:
#   scripts/release-version.sh  tag grammar + per-platform version formats
#   scripts/release.sh          the one command a maintainer runs to release
#   scripts/release-notes.sh    the Google Play "what's new" text of a tag
#
# release.sh cases run against a throwaway clone of a throwaway bare
# "origin", so nothing here can push anywhere real.
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly VERSION_SCRIPT="$REPO_ROOT/scripts/release-version.sh"
readonly RELEASE_SCRIPT="$REPO_ROOT/scripts/release.sh"
readonly NOTES_SCRIPT="$REPO_ROOT/scripts/release-notes.sh"

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

# True when MSIX version $1 is strictly lower than $2, field by field.
msix_lt() {
    awk -v a="$1" -v b="$2" 'BEGIN {
        split(a, x, "."); split(b, y, ".")
        for (i = 1; i <= 4; i++) { if (x[i] + 0 < y[i] + 0) exit 0; if (x[i] + 0 > y[i] + 0) exit 1 }
        exit 1
    }'
}

# MSIX (Microsoft Store): Major.Minor.Build.0, every field at most 65535, the
# revision reserved by the Store. MSIX has no prerelease notation, so the
# order is folded into Build (patch*400 + rank*100 + N) and a Store update
# can never go backwards.
test_msix_versions_encode_the_release_order() {
    [ "$(version v0.2.0-alpha.1 msix)" = '0.2.1.0' ] || fail 'msix of an alpha tag'
    [ "$(version v0.2.0-beta.1 msix)" = '0.2.101.0' ] || fail 'msix of a beta tag'
    [ "$(version v0.2.0-rc.2 msix)" = '0.2.202.0' ] || fail 'msix of an rc tag'
    [ "$(version v0.2.0 msix)" = '0.2.300.0' ] || fail 'msix of a final tag'
    [ "$(version v1.2.3 msix)" = '1.2.1500.0' ] || fail 'msix of a later patch'

    local ordered=(v0.1.0 v0.2.0-alpha.1 v0.2.0-alpha.99 v0.2.0-beta.1 v0.2.0-beta.10 v0.2.0-rc.1
        v0.2.0-rc.99 v0.2.0 v0.2.1-alpha.1 v0.2.1 v0.10.0 v1.0.0)
    local i
    for ((i = 1; i < ${#ordered[@]}; i++)); do
        msix_lt "$(version "${ordered[i-1]}" msix)" "$(version "${ordered[i]}" msix)" \
            || fail "msix order: ${ordered[i-1]} must sort before ${ordered[i]}"
    done

    [ "$(version v0.0.163 msix)" = '0.0.65500.0' ] || fail 'the largest patch that still fits'
    local tag
    for tag in v0.0.164-alpha.1 v0.2.0-beta.100 v65536.0.0 v0.65536.0 v0.2 v0.2.0-preview.1; do
        if version "$tag" msix >/dev/null 2>&1; then fail "accepted a tag MSIX cannot hold: $tag"; fi
    done
}

# Android: versionCode is one integer that must always grow, at most
# 2100000000 (Play's ceiling). Encoded as major*1e7 + minor*1e5 + patch*400 +
# rank*100 + N, the same rank/N folding as MSIX.
test_android_version_codes_encode_the_release_order() {
    [ "$(version v0.2.0-alpha.1 android-code)" = '200001' ] || fail 'android-code of an alpha tag'
    [ "$(version v0.2.0-beta.1 android-code)" = '200101' ] || fail 'android-code of a beta tag'
    [ "$(version v0.2.0-rc.2 android-code)" = '200202' ] || fail 'android-code of an rc tag'
    [ "$(version v0.2.0 android-code)" = '200300' ] || fail 'android-code of a final tag'
    [ "$(version v1.2.3 android-code)" = '10201500' ] || fail 'android-code of a later version'

    local ordered=(v0.1.0 v0.2.0-alpha.1 v0.2.0-alpha.99 v0.2.0-beta.1 v0.2.0-rc.99 v0.2.0
        v0.2.1-alpha.1 v0.2.249 v0.10.0 v0.99.0 v1.0.0 v10.0.0)
    local i
    for ((i = 1; i < ${#ordered[@]}; i++)); do
        (( $(version "${ordered[i-1]}" android-code) < $(version "${ordered[i]}" android-code) )) \
            || fail "android-code order: ${ordered[i-1]} must sort before ${ordered[i]}"
    done

    [ "$(version v209.99.249 android-code)" = '2099999900' ] || fail 'the largest version that still fits'
    local tag
    for tag in v0.100.0 v0.0.250 v210.0.0 v0.2.0-beta.100 v0.2 v0.2.0-preview.1; do
        if version "$tag" android-code >/dev/null 2>&1; then fail "accepted a tag versionCode cannot hold: $tag"; fi
    done
}

# Which Google Play track a tag ships to: alpha to internal testing, beta and
# rc to closed testing (the Play API calls that track "alpha"), a final release
# to production.
test_play_tracks_follow_the_prerelease_word() {
    [ "$(version v0.2.0-alpha.3 play-track)" = 'internal' ] || fail 'alpha track'
    [ "$(version v0.2.0-beta.1 play-track)" = 'alpha' ] || fail 'beta track'
    [ "$(version v0.2.0-rc.1 play-track)" = 'alpha' ] || fail 'rc track'
    [ "$(version v0.2.0 play-track)" = 'production' ] || fail 'final track'
    if version v0.2 play-track >/dev/null 2>&1; then fail 'play-track accepted a malformed tag'; fi
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

# --- release-notes.sh -------------------------------------------------------

notes() { (cd "$sandbox/clone" && bash "$NOTES_SCRIPT" "$@"); }
commit() { git -C "$sandbox/clone" commit -q --allow-empty -m "$1"; }
tag() { git -C "$sandbox/clone" tag -a "$1" -m "$1"; }

test_notes_list_android_facing_changes_since_the_previous_tag() {
    make_sandbox
    tag v0.2.0-beta.1
    commit 'feat(android): read saved comments (#12)'
    commit 'feat(windows): share sheet (#13)'
    commit 'fix(form): accented text in form fields (#14)'
    commit 'feat: show the version in Home (#15)'
    commit 'feat(core): expose comments through FFI (#16)'
    commit 'fix(ci): drop a mirror (#17)'
    commit 'docs(store): add art (#18)'
    tag v0.2.0-beta.2
    commit 'feat(android): not released yet (#19)'
    local expected
    expected="$(printf '%s\n' '• Read saved comments' '• Accented text in form fields' '• Show the version in Home')"
    [ "$(notes v0.2.0-beta.2)" = "$expected" ] || fail "notes were: $(notes v0.2.0-beta.2)"
}

test_notes_fall_back_when_nothing_android_facing_changed() {
    make_sandbox
    tag v0.2.0-beta.1
    commit 'feat(linux): something (#1)'
    tag v0.2.0-beta.2
    [ "$(notes v0.2.0-beta.2)" = 'Bug fixes and improvements.' ] || fail 'no fallback note'
}

test_notes_of_the_first_tag_cover_all_history() {
    make_sandbox
    commit 'feat(android): first feature (#1)'
    tag v0.1.0
    [ "$(notes v0.1.0)" = '• First feature' ] || fail "notes were: $(notes v0.1.0)"
}

# Google Play refuses release notes over 500 characters per language.
test_notes_fit_the_play_limit_on_whole_lines() {
    make_sandbox
    tag v0.2.0-beta.1
    local i
    for ((i = 1; i <= 40; i++)); do commit "feat(android): a reasonably long feature description number $i"; done
    tag v0.2.0-beta.2
    local out
    out="$(notes v0.2.0-beta.2)"
    (( ${#out} <= 500 )) || fail "notes are ${#out} characters"
    [ "$(printf '%s\n' "$out" | head -n1)" = '• A reasonably long feature description number 1' ] || fail 'lost the first line'
    printf '%s\n' "$out" | grep -qvx '• A reasonably long feature description number [0-9]*' && fail 'cut a line in half'
    return 0
}

test_notes_refuse_an_unknown_tag() {
    make_sandbox
    if notes v9.9.9 >/dev/null 2>&1; then fail 'wrote notes for a tag that does not exist'; fi
}

test_required_assets_exist() {
    assert_file "$VERSION_SCRIPT"
    assert_file "$RELEASE_SCRIPT"
    assert_file "$NOTES_SCRIPT"
}

test_tag_maps_to_each_format
test_debian_versions_sort_like_the_releases
test_msix_versions_encode_the_release_order
test_android_version_codes_encode_the_release_order
test_play_tracks_follow_the_prerelease_word
test_malformed_tags_and_formats_fail
test_versions_compare_in_release_order
test_release_tags_origin_main_and_pushes
test_release_ignores_local_commits_and_branches
test_release_refuses_existing_or_older_versions
test_release_refuses_malformed_versions
test_release_asks_before_pushing
test_notes_list_android_facing_changes_since_the_previous_tag
test_notes_fall_back_when_nothing_android_facing_changed
test_notes_of_the_first_tag_cover_all_history
test_notes_fit_the_play_limit_on_whole_lines
test_notes_refuse_an_unknown_tag
test_required_assets_exist
printf 'release tooling shell tests: %d passed, %d skipped\n' $((18 - skipped)) "$skipped"
