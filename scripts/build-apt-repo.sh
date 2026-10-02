#!/usr/bin/env bash
# Builds the signed APT repository linux-release.yml publishes to GitHub
# Pages, from every vitela .deb in <deb-dir>:
#
#   <out-dir>/pool/main/v/vitela/vitela_<version>_amd64.deb
#   <out-dir>/dists/stable/{Release,InRelease,Release.gpg}
#   <out-dir>/dists/stable/main/binary-amd64/Packages{,.gz}
#   <out-dir>/vitela-archive-keyring.gpg   public key, binary (for Signed-By)
#   <out-dir>/vitela.sources               deb822 source for sources.list.d
#
# The repo is rebuilt from scratch on every release, so <out-dir> must not
# exist or must be empty: stale indexes are never patched in place.
#
# Environment:
#   APT_SIGNING_KEY_ID  fingerprint of a secret key already in $GNUPGHOME
#   APT_REPO_URL        URL the repo is served from (written into vitela.sources)
#   APT_FTPARCHIVE      apt-ftparchive to use (default: from PATH)
set -euo pipefail

readonly SUITE='stable'
readonly COMPONENT='main'
readonly ARCH='amd64'
readonly PACKAGE='vitela'
readonly KEYRING_NAME='vitela-archive-keyring.gpg'
readonly APT_FTPARCHIVE="${APT_FTPARCHIVE:-apt-ftparchive}"

fail() { printf 'build-apt-repo: %s\n' "$*" >&2; exit 1; }
require_tool() { command -v "$1" >/dev/null 2>&1 || fail "required tool not found: $1"; }

[ "$#" -eq 2 ] || fail 'usage: build-apt-repo.sh <deb-dir> <out-dir>'
readonly DEB_DIR="$1"
readonly OUT_DIR="$2"
readonly KEY_ID="${APT_SIGNING_KEY_ID:-}"
readonly REPO_URL="${APT_REPO_URL:-}"

[ -n "$KEY_ID" ] || fail 'APT_SIGNING_KEY_ID is not set'
[ -n "$REPO_URL" ] || fail 'APT_REPO_URL is not set'
require_tool "$APT_FTPARCHIVE"
require_tool gpg
require_tool dpkg-deb
require_tool gzip
[ -d "$DEB_DIR" ] || fail "package directory not found: $DEB_DIR"
if [ -e "$OUT_DIR" ]; then
    [ -d "$OUT_DIR" ] && [ -z "$(find "$OUT_DIR" -mindepth 1 -print -quit)" ] \
        || fail "output directory exists and is not empty: $OUT_DIR"
fi
gpg --batch --list-secret-keys "$KEY_ID" >/dev/null 2>&1 || fail "no secret key for APT_SIGNING_KEY_ID in the keyring"

# Validate every package before anything is written.
declare -A seen_versions=()
declare -a debs=()
while IFS= read -r -d '' deb; do
    package="$(dpkg-deb -f "$deb" Package)" || fail "unreadable package: $deb"
    version="$(dpkg-deb -f "$deb" Version)"
    arch="$(dpkg-deb -f "$deb" Architecture)"
    [ "$package" = "$PACKAGE" ] || fail "refusing to publish package '$package' from $deb"
    [ "$arch" = "$ARCH" ] || fail "refusing to publish architecture '$arch' from $deb"
    [ -z "${seen_versions[$version]:-}" ] || fail "version $version appears twice: ${seen_versions[$version]} and $deb"
    seen_versions[$version]="$deb"
    debs+=("$deb")
done < <(find "$DEB_DIR" -maxdepth 1 -type f -name '*.deb' -print0 | sort -z)
[ "${#debs[@]}" -gt 0 ] || fail "no .deb packages in $DEB_DIR"

# Assemble next to the destination and move it into place only once signed.
staging="$(mktemp -d "$(dirname "$OUT_DIR")/.apt-repo.XXXXXX")"
trap 'rm -rf -- "$staging"' EXIT

pool="pool/$COMPONENT/${PACKAGE:0:1}/$PACKAGE"
binary_dir="dists/$SUITE/$COMPONENT/binary-$ARCH"
mkdir -p "$staging/$pool" "$staging/$binary_dir"
for deb in "${debs[@]}"; do
    # Name pool files from the control fields, not the input filename:
    # GitHub release assets come back with '~' rewritten to '.'.
    version="$(dpkg-deb -f "$deb" Version)"
    install -m644 "$deb" "$staging/$pool/${PACKAGE}_${version}_${ARCH}.deb"
done

(
    cd "$staging"
    "$APT_FTPARCHIVE" packages "pool/$COMPONENT" > "$binary_dir/Packages"
    gzip -9n --keep "$binary_dir/Packages"
    "$APT_FTPARCHIVE" \
        -o APT::FTPArchive::Release::Origin=Vitela \
        -o APT::FTPArchive::Release::Label=Vitela \
        -o APT::FTPArchive::Release::Suite="$SUITE" \
        -o APT::FTPArchive::Release::Codename="$SUITE" \
        -o APT::FTPArchive::Release::Architectures="$ARCH" \
        -o APT::FTPArchive::Release::Components="$COMPONENT" \
        -o APT::FTPArchive::Release::Description='Vitela PDF editor' \
        release "dists/$SUITE" > release.tmp
    mv release.tmp "dists/$SUITE/Release"
)
grep -q "^Package: $PACKAGE\$" "$staging/$binary_dir/Packages" || fail 'apt-ftparchive produced an empty Packages index'

gpg --batch --yes --local-user "$KEY_ID" --digest-algo SHA512 \
    --clearsign --output "$staging/dists/$SUITE/InRelease" "$staging/dists/$SUITE/Release"
gpg --batch --yes --local-user "$KEY_ID" --digest-algo SHA512 \
    --armor --detach-sign --output "$staging/dists/$SUITE/Release.gpg" "$staging/dists/$SUITE/Release"
gpg --batch --export "$KEY_ID" > "$staging/$KEYRING_NAME"
[ -s "$staging/$KEYRING_NAME" ] || fail 'exported public keyring is empty'

cat > "$staging/$PACKAGE.sources" <<EOF
Types: deb
URIs: $REPO_URL
Suites: $SUITE
Components: $COMPONENT
Architectures: $ARCH
Signed-By: /etc/apt/keyrings/$KEYRING_NAME
EOF

[ ! -e "$OUT_DIR" ] || rmdir -- "$OUT_DIR"
mv -- "$staging" "$OUT_DIR"
trap - EXIT
chmod 755 "$OUT_DIR"
printf 'build-apt-repo: published %d package(s) to %s\n' "${#debs[@]}" "$OUT_DIR"
