#!/usr/bin/env bash
# FilePath: scripts/release/signing-identity.sh
# Manages the self-signed code signing certificate that releases are signed with.
#
# macOS keeps Microphone, Accessibility and Speech Recognition grants against the app's
# designated requirement. An ad-hoc signature has no certificate, so that requirement is the
# binary's hash and every upgrade loses the grants; with a certificate it is the bundle
# identifier plus the certificate, which stays the same from one release to the next. The
# certificate needs no Apple account and no trust setting: codesign signs with it as long as it
# is in a keychain on the search list, and nothing checks its trust when a grant is matched.
#
# Usage:
#   scripts/release/signing-identity.sh create BACKUP.p12   make the certificate, save a backup
#   scripts/release/signing-identity.sh restore BACKUP.p12  import a backup on another Mac
#   scripts/release/signing-identity.sh show                print the identity build.sh signs with
#
# The private key never leaves the keychain except in the passphrase-protected backup. Losing
# both means the next release has a new certificate and every user grants permissions once more.
# SIGNING_KEYCHAIN overrides the login keychain.
set -euo pipefail

name="Simple Voice Release"
keychain="${SIGNING_KEYCHAIN:-${HOME}/Library/Keychains/login.keychain-db}"
# LibreSSL writes the PKCS#12 ciphers that `security import` reads; OpenSSL 3 does not by default.
openssl=/usr/bin/openssl

usage() {
    echo "usage: $0 create BACKUP.p12 | restore BACKUP.p12 | show" >&2
    exit 1
}

has_identity() {
    security find-certificate -c "$name" "$keychain" >/dev/null 2>&1
}

work=""
cleanup() {
    if [ -n "$work" ]; then
        rm -rf "$work"
    fi
}
trap cleanup EXIT

create() {
    local backup="$1"
    if has_identity; then
        echo "signing-identity: \"${name}\" already exists in ${keychain}; nothing created" >&2
        exit 1
    fi
    if [ -e "$backup" ]; then
        echo "signing-identity: ${backup} already exists; choose another backup path" >&2
        exit 1
    fi
    work="$(mktemp -d)"
    chmod 700 "$work"
    cat >"${work}/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = ${name}
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
EOF
    # `security import -f openssl` reads the traditional RSA key format that genrsa writes.
    "$openssl" genrsa -out "${work}/key.pem" 3072 2>/dev/null
    # Twenty years: codesign refuses an expired certificate, and a new one costs every user a
    # fresh round of permission grants.
    "$openssl" req -x509 -new -key "${work}/key.pem" -days 7305 -config "${work}/cert.cnf" \
        -out "${work}/cert.pem"
    security import "${work}/key.pem" -k "$keychain" -t priv -f openssl -T /usr/bin/codesign \
        >/dev/null
    security import "${work}/cert.pem" -k "$keychain" -t cert >/dev/null
    echo "signing-identity: \"${name}\" is in ${keychain}" >&2
    echo "signing-identity: choose a passphrase for the backup and keep it with the file" >&2
    "$openssl" pkcs12 -export -inkey "${work}/key.pem" -in "${work}/cert.pem" -name "$name" \
        -out "$backup"
    chmod 600 "$backup"
    echo "signing-identity: backup written to ${backup}" >&2
    show
}

restore() {
    local backup="$1"
    if has_identity; then
        echo "signing-identity: \"${name}\" already exists in ${keychain}; nothing imported" >&2
        exit 1
    fi
    # Without -P, macOS asks for the backup's passphrase in a dialog, so it never lands in argv.
    security import "$backup" -k "$keychain" -f pkcs12 -T /usr/bin/codesign >/dev/null
    show
}

show() {
    if ! has_identity; then
        echo "signing-identity: no \"${name}\" in ${keychain}; run create or restore" >&2
        exit 1
    fi
    local sha1
    sha1="$(security find-certificate -c "$name" -Z "$keychain" | sed -n 's/^SHA-1 hash: //p')"
    echo "${name} (SHA-1 ${sha1})"
}

[ "$#" -ge 1 ] || usage
case "$1" in
create | restore)
    [ "$#" -eq 2 ] || usage
    "$1" "$2"
    ;;
show)
    [ "$#" -eq 1 ] || usage
    show
    ;;
*) usage ;;
esac
