#!/usr/bin/env bash
# Import the identity Adroit.app is signed with into a keychain, so that
# bundle.sh finds it by name: the login keychain by default, so a local build
# is signed as releases are and keeps what macOS let an installed release do;
# a new keychain of its own when one is named, as the release does.
#
#   MACOS_SIGNING_P12 (identity.p12 in base64) and MACOS_SIGNING_PASSWORD in
#   the environment, as make-identity.sh made them;
#   import-identity.sh [<keychain to create>]
set -euo pipefail

name="Adroit Release"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
printf '%s' "$MACOS_SIGNING_P12" | /usr/bin/base64 -D >"$work/identity.p12"

if [ $# -gt 0 ]; then
  keychain="$1"
  password="$(/usr/bin/openssl rand -hex 24)"
  security create-keychain -p "$password" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "$password" "$keychain"
  security import "$work/identity.p12" -k "$keychain" -P "$MACOS_SIGNING_PASSWORD" -T /usr/bin/codesign
  # Lets codesign use the key without asking.
  security set-key-partition-list -S apple-tool:,apple: -s -k "$password" "$keychain" >/dev/null
  # shellcheck disable=SC2046 # one keychain path per line, none with spaces
  security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
  exit 0
fi

keychain="$HOME/Library/Keychains/login.keychain-db"
printf '%s' "$MACOS_SIGNING_PASSWORD" >"$work/password"
/usr/bin/openssl pkcs12 -in "$work/identity.p12" -passin "file:$work/password" -nokeys -out "$work/cert.pem" 2>/dev/null
hash="$(/usr/bin/openssl x509 -in "$work/cert.pem" -noout -fingerprint -sha1 | cut -d= -f2 | tr -d :)"
# Matched by hash, not name: a certificate of the same name may be one the
# identity has since replaced, or lack its private key.
if security find-identity -p codesigning "$keychain" | grep -q "$hash"; then
  echo "\"$name\" is already in the login keychain"
  exit 0
fi
if security find-certificate -c "$name" "$keychain" >/dev/null 2>&1; then
  echo "the login keychain holds a \"$name\" other than the one given; delete it in Keychain Access first" >&2
  exit 1
fi
security import "$work/identity.p12" -k "$keychain" -P "$MACOS_SIGNING_PASSWORD" -T /usr/bin/codesign
echo "imported \"$name\" into the login keychain"
