#!/usr/bin/env bash
# Import the identity make-identity.sh made into the login keychain, so that
# bundle.sh signs the app with it.
#
#   import-identity.sh <folder make-identity.sh made>
set -euo pipefail

in="$1"
name="Adroit Release"
keychain="$HOME/Library/Keychains/login.keychain-db"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
/usr/bin/openssl pkcs12 -in "$in/identity.p12" -passin "file:$in/password" -nokeys -out "$work/cert.pem" 2>/dev/null
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
security import "$in/identity.p12" -k "$keychain" -P "$(cat "$in/password")" -T /usr/bin/codesign
echo "imported \"$name\" into the login keychain"
