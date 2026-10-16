#!/bin/sh
# Installs Vitela on Debian, Ubuntu and their derivatives (x86_64):
#
#   curl -fsSL @VITELA_REPO_URL@/install.sh | sh
#
# It adds the signed APT repository and installs the vitela package; from then
# on `apt upgrade` keeps it current, so this runs once per machine. Running it
# again is harmless. build-apt-repo.sh publishes this file next to the
# repository, with @VITELA_REPO_URL@ replaced by the repository's URL.
#
# POSIX sh, not bash: `| sh` is dash on Debian and Ubuntu. Everything lives in
# main(), called on the last line, so a download cut short defines a function
# that is never called and runs nothing.
#
# VITELA_INSTALL_ROOT prefixes every path written (tests only; default: /).

main() {
    set -eu
    repo_url='@VITELA_REPO_URL@'
    releases_url='https://github.com/PabloBaroParra/vitela/releases'
    keyring_dir="${VITELA_INSTALL_ROOT:-}/etc/apt/keyrings"
    sources_dir="${VITELA_INSTALL_ROOT:-}/etc/apt/sources.list.d"

    [ "$(uname -s)" = Linux ] || unsupported 'this installer is for Linux'
    command -v apt-get >/dev/null 2>&1 && command -v dpkg >/dev/null 2>&1 \
        || unsupported 'apt-get was not found; only Debian, Ubuntu and their derivatives are supported'
    arch="$(dpkg --print-architecture)"
    [ "$arch" = amd64 ] || unsupported "the APT repository only has amd64 builds, not $arch"

    if [ "$(id -u)" -eq 0 ]; then
        sudo=''
    elif command -v sudo >/dev/null 2>&1; then
        sudo='sudo'
    else
        fail 'run this as root, or install sudo first'
    fi

    if command -v curl >/dev/null 2>&1; then
        fetch() { curl -fsSL "$1" -o "$2"; }
    elif command -v wget >/dev/null 2>&1; then
        fetch() { wget -q -O "$2" "$1"; }
    else
        fail 'curl or wget is required'
    fi

    tmp="$(mktemp -d)"
    trap 'rm -rf -- "$tmp"' EXIT
    fetch "$repo_url/vitela-archive-keyring.gpg" "$tmp/keyring.gpg" \
        || fail "could not download the signing key from $repo_url"
    fetch "$repo_url/vitela.sources" "$tmp/vitela.sources" \
        || fail "could not download the repository source from $repo_url"
    [ -s "$tmp/keyring.gpg" ] && [ -s "$tmp/vitela.sources" ] \
        || fail 'the downloaded repository files are empty'

    say 'adding the Vitela APT repository'
    $sudo install -d -m 0755 "$keyring_dir" "$sources_dir"
    $sudo install -m 0644 "$tmp/keyring.gpg" "$keyring_dir/vitela-archive-keyring.gpg"
    $sudo install -m 0644 "$tmp/vitela.sources" "$sources_dir/vitela.sources"

    say 'installing vitela'
    $sudo apt-get update
    $sudo apt-get install -y vitela
    say 'done. `sudo apt upgrade` keeps Vitela up to date from now on.'
}

say() { printf 'vitela install: %s\n' "$*"; }
fail() { printf 'vitela install: %s\n' "$*" >&2; exit 1; }
unsupported() { fail "$1. Download the AppImage from $releases_url instead."; }

main "$@"
