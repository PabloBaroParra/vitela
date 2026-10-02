#!/usr/bin/env bash
# Shell tests for the signed APT repository that linux-release.yml publishes:
# scripts/build-apt-repo.sh (.deb pool -> signed dists/ tree). The end-to-end
# cases drive a real, unprivileged `apt-get update` against the built tree,
# so "signed" means "apt accepts it", not "a .gpg file exists". The repo is
# served over loopback HTTP, as Pages serves it: apt reads a file:// pool in
# place and would never prove the pool files are fetchable by URL.
#
# APT_FTPARCHIVE may point at an apt-ftparchive that is not on PATH (apt-utils
# extracted without root), the same override build-apt-repo.sh honours.
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
readonly BUILD_SCRIPT="$REPO_ROOT/scripts/build-apt-repo.sh"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
assert_file() { [ -f "$1" ] || fail "expected file: $1"; }
assert_not_exists() { [ ! -e "$1" ] || fail "unexpected path: $1"; }
assert_contains() { grep -Fqx -- "$2" "$1" || fail "expected line '$2' in $1"; }

fixture_root=''
server_pid=''
cleanup() {
    [ -z "$server_pid" ] || kill "$server_pid" 2>/dev/null || true
    server_pid=''
    [ -z "$fixture_root" ] && return
    GNUPGHOME="$fixture_root/gnupg-a" gpgconf --kill gpg-agent 2>/dev/null || true
    GNUPGHOME="$fixture_root/gnupg-b" gpgconf --kill gpg-agent 2>/dev/null || true
    rm -rf -- "$fixture_root"
}
trap cleanup EXIT

new_key() {
    local home="$1"
    mkdir -m 700 -p "$home"
    GNUPGHOME="$home" gpg --batch --quiet --passphrase '' \
        --quick-gen-key "Vitela test archive <archive@vitela.invalid>" ed25519 sign never 2>/dev/null
    GNUPGHOME="$home" gpg --batch --with-colons --list-secret-keys | awk -F: '/^fpr:/ { print $10; exit }'
}

make_deb() {
    local package="$1" version="$2" arch="$3" out_dir="$4"
    local root="$fixture_root/deb-src/$package-$version-$arch"
    mkdir -p "$root/DEBIAN" "$root/usr/share/doc/$package"
    printf '%s\n' "$version" > "$root/usr/share/doc/$package/version"
    printf '%s\n' \
        "Package: $package" "Version: $version" "Architecture: $arch" \
        'Maintainer: Vitela contributors <vitela@invalid>' 'Description: fixture' ' fixture' \
        > "$root/DEBIAN/control"
    mkdir -p "$out_dir"
    dpkg-deb --build --root-owner-group "$root" "$out_dir/${package}_${version}_${arch}.deb" >/dev/null
}

make_fixture() {
    cleanup
    # No space here, unlike test-package-linux.sh: deb822 reads a spaced
    # Signed-By as a fingerprint list.
    fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/vitela-apt.XXXXXX")"
    key_a="$(new_key "$fixture_root/gnupg-a")"
    make_deb vitela '0.1.0~beta.1' amd64 "$fixture_root/debs"
    make_deb vitela '0.1.0~beta.2' amd64 "$fixture_root/debs"
    start_server
}

start_server() {
    local port attempt
    port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
    python3 -m http.server "$port" --bind 127.0.0.1 --directory "$fixture_root" >/dev/null 2>&1 &
    server_pid=$!
    for attempt in $(seq 50); do
        (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null && break
        sleep 0.1
    done
    repo_url="http://127.0.0.1:$port/repo"
}

build_repo() {
    GNUPGHOME="$fixture_root/gnupg-a" APT_SIGNING_KEY_ID="$key_a" \
        APT_REPO_URL="$repo_url" \
        bash "$BUILD_SCRIPT" "$fixture_root/debs" "$fixture_root/repo"
}

# An apt that only knows about the fixture repo: its own state/cache/sources,
# and the source's Signed-By pointing at whichever keyring the case supplies.
fixture_apt() {
    local keyring="$1"; shift
    local apt_root="$fixture_root/apt"
    mkdir -p "$apt_root/lists/partial" "$apt_root/cache/archives/partial" "$apt_root/sources.list.d"
    : > "$apt_root/status"
    sed "s|^Signed-By: .*|Signed-By: $keyring|" "$fixture_root/repo/vitela.sources" \
        > "$apt_root/sources.list.d/vitela.sources"
    apt-get -q \
        -o Dir::State::Lists="$apt_root/lists" \
        -o Dir::State::status="$apt_root/status" \
        -o Dir::Cache="$apt_root/cache" \
        -o Dir::Etc::SourceList=/dev/null \
        -o Dir::Etc::SourceParts="$apt_root/sources.list.d" \
        -o Dir::Etc::Trusted=/dev/null \
        -o Dir::Etc::TrustedParts=/nonexistent \
        -o Debug::NoLocking=1 \
        -o APT::Sandbox::User="$(id -un)" \
        "$@"
}

fixture_apt_cache() {
    local apt_root="$fixture_root/apt"
    apt-cache \
        -o Dir::State::Lists="$apt_root/lists" \
        -o Dir::State::status="$apt_root/status" \
        -o Dir::Cache="$apt_root/cache" \
        -o Dir::Etc::SourceList=/dev/null \
        -o Dir::Etc::SourceParts="$apt_root/sources.list.d" \
        "$@"
}

# Captured, never piped into `grep -q`: under pipefail, grep closing the pipe
# early SIGPIPEs apt-cache and the whole check reads as false.
candidate() {
    local policy
    policy="$(fixture_apt_cache policy vitela 2>/dev/null)" || true
    awk '/Candidate:/ { print $2; exit }' <<< "$policy"
}

test_built_repo_has_signed_indexes_and_install_files() {
    make_fixture
    build_repo
    local repo="$fixture_root/repo"
    assert_file "$repo/dists/stable/Release"
    assert_file "$repo/dists/stable/InRelease"
    assert_file "$repo/dists/stable/Release.gpg"
    assert_file "$repo/dists/stable/main/binary-amd64/Packages"
    assert_file "$repo/dists/stable/main/binary-amd64/Packages.gz"
    assert_file "$repo/pool/main/v/vitela/vitela_0.1.0~beta.1_amd64.deb"
    assert_file "$repo/pool/main/v/vitela/vitela_0.1.0~beta.2_amd64.deb"
    assert_contains "$repo/dists/stable/Release" 'Suite: stable'
    assert_contains "$repo/dists/stable/Release" 'Architectures: amd64'
    assert_contains "$repo/dists/stable/Release" 'Components: main'
    assert_contains "$repo/vitela.sources" "URIs: $repo_url"
    assert_contains "$repo/vitela.sources" 'Signed-By: /etc/apt/keyrings/vitela-archive-keyring.gpg'
    gpgv --quiet --keyring "$repo/vitela-archive-keyring.gpg" "$repo/dists/stable/InRelease" 2>/dev/null \
        || fail 'InRelease does not verify against the published keyring'
    gpgv --quiet --keyring "$repo/vitela-archive-keyring.gpg" "$repo/dists/stable/Release.gpg" "$repo/dists/stable/Release" 2>/dev/null \
        || fail 'Release.gpg does not verify against the published keyring'
    if gpg --batch --list-packets "$repo/vitela-archive-keyring.gpg" 2>/dev/null | grep -q 'secret key packet'; then
        fail 'published keyring contains secret key material'
    fi
}

test_apt_installs_the_newest_version_from_the_signed_repo() {
    make_fixture
    build_repo
    fixture_apt "$fixture_root/repo/vitela-archive-keyring.gpg" update >/dev/null 2>"$fixture_root/apt-update.err" \
        || { cat "$fixture_root/apt-update.err" >&2; fail 'apt-get update rejected the signed repo'; }
    grep -q '^W:' "$fixture_root/apt-update.err" && { cat "$fixture_root/apt-update.err" >&2; fail 'apt-get update warned'; }
    [ "$(candidate)" = '0.1.0~beta.2' ] || fail 'apt did not pick the newest beta'
    fixture_apt "$fixture_root/repo/vitela-archive-keyring.gpg" -y install --download-only vitela >/dev/null 2>&1 \
        || fail 'apt could not download the package from the pool'
    assert_file "$fixture_root/apt/cache/archives/vitela_0.1.0~beta.2_amd64.deb"
}

test_apt_rejects_the_repo_under_a_different_key() {
    make_fixture
    build_repo
    new_key "$fixture_root/gnupg-b" >/dev/null
    GNUPGHOME="$fixture_root/gnupg-b" gpg --batch --export > "$fixture_root/other-keyring.gpg"
    if fixture_apt "$fixture_root/other-keyring.gpg" update >/dev/null 2>&1 \
        && [ "$(candidate)" != '(none)' ] && [ -n "$(candidate)" ]; then
        fail 'apt trusted the repo under a key that did not sign it'
    fi
}

test_tampered_index_is_rejected() {
    make_fixture
    build_repo
    printf '\n' >> "$fixture_root/repo/dists/stable/main/binary-amd64/Packages"
    rm -f "$fixture_root/repo/dists/stable/main/binary-amd64/Packages.gz"
    if fixture_apt "$fixture_root/repo/vitela-archive-keyring.gpg" update >/dev/null 2>&1 \
        && [ "$(candidate)" != '(none)' ] && [ -n "$(candidate)" ]; then
        fail 'apt accepted a Packages index that no longer matches the signed Release'
    fi
}

test_bad_inputs_fail_before_writing_the_repo() {
    make_fixture
    if GNUPGHOME="$fixture_root/gnupg-a" APT_REPO_URL="$repo_url" \
        bash "$BUILD_SCRIPT" "$fixture_root/debs" "$fixture_root/repo" >/dev/null 2>&1; then
        fail 'built a repo without a signing key id'
    fi
    assert_not_exists "$fixture_root/repo"

    if GNUPGHOME="$fixture_root/gnupg-a" APT_SIGNING_KEY_ID="$key_a" \
        bash "$BUILD_SCRIPT" "$fixture_root/debs" "$fixture_root/repo" >/dev/null 2>&1; then
        fail 'built a repo without a repo URL'
    fi
    assert_not_exists "$fixture_root/repo"

    if GNUPGHOME="$fixture_root/gnupg-a" APT_SIGNING_KEY_ID='0000000000000000000000000000000000000000' \
        APT_REPO_URL="$repo_url" \
        bash "$BUILD_SCRIPT" "$fixture_root/debs" "$fixture_root/repo" >/dev/null 2>&1; then
        fail 'built a repo with a signing key that is not in the keyring'
    fi
    assert_not_exists "$fixture_root/repo"

    mkdir -p "$fixture_root/empty"
    if GNUPGHOME="$fixture_root/gnupg-a" APT_SIGNING_KEY_ID="$key_a" APT_REPO_URL="$repo_url" \
        bash "$BUILD_SCRIPT" "$fixture_root/empty" "$fixture_root/repo" >/dev/null 2>&1; then
        fail 'built a repo with no packages'
    fi
    assert_not_exists "$fixture_root/repo"

    mkdir -p "$fixture_root/repo"
    printf 'stale\n' > "$fixture_root/repo/leftover"
    if build_repo >/dev/null 2>&1; then fail 'built into a non-empty output directory'; fi
    rm -rf -- "$fixture_root/repo"
}

test_foreign_or_duplicate_packages_fail() {
    make_fixture
    make_deb not-vitela 1.0 amd64 "$fixture_root/debs"
    if build_repo >/dev/null 2>&1; then fail 'published a package that is not vitela'; fi
    assert_not_exists "$fixture_root/repo"

    make_fixture
    make_deb vitela 0.1.0~beta.3 arm64 "$fixture_root/debs"
    if build_repo >/dev/null 2>&1; then fail 'published a package for an unsupported architecture'; fi
    assert_not_exists "$fixture_root/repo"

    make_fixture
    # GitHub renames release assets ('~' -> '.'), so the same version can
    # reach the pool under two different filenames.
    cp "$fixture_root/debs/vitela_0.1.0~beta.1_amd64.deb" "$fixture_root/debs/vitela_0.1.0.beta.1_amd64.deb"
    if build_repo >/dev/null 2>&1; then fail 'published the same version twice'; fi
    assert_not_exists "$fixture_root/repo"
}

test_required_assets_exist() {
    assert_file "$BUILD_SCRIPT"
    assert_file "$REPO_ROOT/.github/workflows/linux-release.yml"
}

test_built_repo_has_signed_indexes_and_install_files
test_apt_installs_the_newest_version_from_the_signed_repo
test_apt_rejects_the_repo_under_a_different_key
test_tampered_index_is_rejected
test_bad_inputs_fail_before_writing_the_repo
test_foreign_or_duplicate_packages_fail
test_required_assets_exist
printf 'apt-repo shell tests: 7 passed\n'
