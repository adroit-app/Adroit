#!/usr/bin/env bash
# Make the self-signed identity the macOS app is signed with, "Adroit
# Release": identity.p12 and the password that opens it, written into a new
# folder. macOS keeps what it let the app do only while every build is signed
# by the same one, so the folder is kept to import it again on another Mac.
#
#   make-identity.sh <folder to create>
set -euo pipefail

out="$1"
name="Adroit Release"
# The folder holds the private key and the password that opens it.
umask 077
mkdir "$out"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cat >"$work/openssl.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $name
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
# The system's LibreSSL writes a PKCS#12 file the keychain can import;
# OpenSSL 3 encrypts it in a way the keychain cannot read.
openssl=/usr/bin/openssl
"$openssl" req -x509 -newkey rsa:2048 -nodes -days 7300 \
  -config "$work/openssl.cnf" -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
"$openssl" rand -hex 24 >"$out/password"
"$openssl" pkcs12 -export -passout "file:$out/password" \
  -inkey "$work/key.pem" -in "$work/cert.pem" -out "$out/identity.p12"
"$openssl" x509 -in "$work/cert.pem" -noout -fingerprint -sha256
cat <<MSG
Import it into the login keychain with import-identity.sh $out, then keep
$out somewhere safe: a new identity makes macOS ask for every permission again.
MSG
