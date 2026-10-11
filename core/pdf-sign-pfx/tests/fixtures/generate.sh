#!/bin/sh
# Regenerates the PKCS#12 fixtures that `tests/legacy_exports.rs` reads, the
# way older OpenSSL/LibreSSL builds export them. Run with macOS's
# /usr/bin/openssl (LibreSSL 3.3.6 when these were made); OpenSSL 3 rejects
# some of the legacy schemes without `-legacy`. Every key here is a throwaway
# test key and every file's password is `password`.
set -eu
cd "$(dirname "$0")"
OPENSSL=${OPENSSL:-/usr/bin/openssl}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

certificate() { # <key type options...>
    "$OPENSSL" req -x509 -nodes -days 36500 -subj "/CN=Vitela legacy export test" \
        -keyout "$work/key.pem" -out "$work/cert.pem" "$@" 2>/dev/null
}
export_p12() { # <file> [pkcs12 -export options...]
    out=$1
    shift
    "$OPENSSL" pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" \
        -passout pass:password -out "$out" "$@"
}

certificate -newkey rsa:2048
# The defaults: RC2-40 for the certificate bag, PKCS#12 3DES for the key.
export_p12 rsa-rc2-40.p12
# PBES2 as LibreSSL writes it: PBKDF2 with HMAC-SHA1, then AES or 3DES.
export_p12 rsa-pbes2-aes-256.p12 -certpbe AES-256-CBC -keypbe AES-256-CBC
export_p12 rsa-pbes2-3des.p12 -certpbe DES-EDE3-CBC -keypbe DES-EDE3-CBC
# A scheme the reader does not support.
export_p12 rsa-rc4-128.p12 -certpbe PBE-SHA1-RC4-128

certificate -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -pkeyopt ec_param_enc:named_curve
export_p12 p256-rc2-40.p12

# Without `ec_param_enc:named_curve` LibreSSL spells the curve out.
certificate -newkey ec -pkeyopt ec_paramgen_curve:prime256v1
export_p12 p256-explicit-curve.p12

certificate -newkey ec -pkeyopt ec_paramgen_curve:secp384r1 -pkeyopt ec_param_enc:named_curve
export_p12 p384.p12
