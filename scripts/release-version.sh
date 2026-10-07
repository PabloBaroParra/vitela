#!/usr/bin/env bash
# The single definition of a Vitela release version. The release tag is the
# source of truth for every platform; this script is the only place that
# parses it, and each platform's release job asks it for its own format:
#
#   release-version.sh v0.2.0-beta.1 semver   ->  0.2.0-beta.1
#   release-version.sh v0.2.0-beta.1 debian   ->  0.2.0~beta.1
#   release-version.sh v0.2.0-beta.1 msix     ->  0.2.101.0
#   release-version.sh v0.2.0-beta.1 android-code  ->  200101
#   release-version.sh v0.2.0-beta.1 play-track    ->  alpha
#   release-version.sh v0.2.0 newer-than v0.2.0-rc.1   (exit 0 if newer)
#
# Tag grammar: vMAJOR.MINOR.PATCH, optionally -alpha.N, -beta.N or -rc.N
# (N >= 1). The prerelease words are a closed set because their order
# (alpha < beta < rc < final) must be expressible in every platform's own
# version scheme, not only in SemVer's.
#
# A platform that gains a release pipeline adds its format here, next to
# the others, so the translations can never drift apart.
set -euo pipefail

fail() { printf 'release-version: %s\n' "$*" >&2; exit 1; }

readonly NUMBER='(0|[1-9][0-9]*)'
readonly PATTERN="^v$NUMBER\.$NUMBER\.$NUMBER(-(alpha|beta|rc)\.([1-9][0-9]*))?$"

# Sets major/minor/patch/word/n for one tag, or fails.
parse() {
    [[ "$1" =~ $PATTERN ]] \
        || fail "'$1' is not vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-(alpha|beta|rc).N"
    major="${BASH_REMATCH[1]}" minor="${BASH_REMATCH[2]}" patch="${BASH_REMATCH[3]}"
    word="${BASH_REMATCH[5]}" n="${BASH_REMATCH[6]}"
}

# Prints "major minor patch rank n", where a final release outranks rc.
sort_key() {
    parse "$1"
    local rank
    case "$word" in
        alpha) rank=0 ;; beta) rank=1 ;; rc) rank=2 ;; '') rank=3 n=0 ;;
    esac
    printf '%s %s %s %s %s\n' "$major" "$minor" "$patch" "$rank" "$n"
}

[ "$#" -ge 2 ] || fail 'usage: release-version.sh <tag> (semver | debian | newer-than <tag>)'
readonly TAG="$1" FORMAT="$2"

case "$FORMAT" in
    semver|debian)
        [ "$#" -eq 2 ] || fail "$FORMAT takes no further arguments"
        parse "$TAG"
        base="$major.$minor.$patch"
        if [ -z "$word" ]; then
            printf '%s\n' "$base"
        elif [ "$FORMAT" = semver ]; then
            printf '%s-%s.%s\n' "$base" "$word" "$n"
        else
            # '~' sorts before anything in dpkg, so a beta precedes its final.
            printf '%s~%s.%s\n' "$base" "$word" "$n"
        fi
        ;;
    msix)
        # Microsoft Store: Major.Minor.Build.0. Every field is 16-bit and the
        # Store reserves the revision, so the prerelease order is folded into
        # Build: patch*400 + rank*100 + N (alpha 0, beta 1, rc 2, final 3).
        # N is capped at 99 so one rank can never spill into the next.
        [ "$#" -eq 2 ] || fail "$FORMAT takes no further arguments"
        # Validate here: a parse failure inside $(sort_key ...) would not exit.
        parse "$TAG"
        read -r major minor patch rank n <<< "$(sort_key "$TAG")"
        (( n <= 99 )) || fail "$TAG: MSIX holds at most 99 prereleases of one kind"
        build=$(( patch * 400 + rank * 100 + n ))
        for field in "$major" "$minor" "$build"; do
            (( field <= 65535 )) || fail "$TAG does not fit an MSIX version (fields are at most 65535)"
        done
        printf '%s.%s.%s.0\n' "$major" "$minor" "$build"
        ;;
    android-code)
        # Google Play versionCode: one integer that must always grow, at most
        # 2100000000. major*1e7 + minor*1e5 + patch*400 + rank*100 + N, with
        # the same rank/N folding as msix; the caps keep each field inside
        # its own decimal slot so no version can overtake a later one.
        [ "$#" -eq 2 ] || fail "$FORMAT takes no further arguments"
        parse "$TAG"
        read -r major minor patch rank n <<< "$(sort_key "$TAG")"
        (( n <= 99 )) || fail "$TAG: versionCode holds at most 99 prereleases of one kind"
        (( minor <= 99 )) || fail "$TAG: versionCode holds a minor version of at most 99"
        (( patch <= 249 )) || fail "$TAG: versionCode holds a patch version of at most 249"
        (( major <= 209 )) || fail "$TAG: versionCode holds a major version of at most 209"
        printf '%s\n' $(( major * 10000000 + minor * 100000 + patch * 400 + rank * 100 + n ))
        ;;
    play-track)
        # Where a release lands on Google Play: alpha in internal testing,
        # beta and rc in closed testing, a final release in production. The
        # Play API names closed testing "alpha" and open testing "beta"; betas
        # go to closed testing because a new personal developer account must
        # run a closed test (12 testers, 14 days) before it may use open
        # testing or production.
        [ "$#" -eq 2 ] || fail "$FORMAT takes no further arguments"
        parse "$TAG"
        case "$word" in
            alpha) printf 'internal\n' ;;
            beta|rc) printf 'alpha\n' ;;
            '') printf 'production\n' ;;
        esac
        ;;
    newer-than)
        [ "$#" -eq 3 ] || fail 'usage: release-version.sh <tag> newer-than <tag>'
        # Validate here: a parse failure inside $(sort_key ...) would not exit.
        parse "$TAG"
        parse "$3"
        read -r -a ours <<< "$(sort_key "$TAG")"
        read -r -a theirs <<< "$(sort_key "$3")"
        for i in 0 1 2 3 4; do
            (( ours[i] > theirs[i] )) && exit 0
            (( ours[i] < theirs[i] )) && exit 1
        done
        exit 1
        ;;
    *) fail "unknown format '$FORMAT'" ;;
esac
