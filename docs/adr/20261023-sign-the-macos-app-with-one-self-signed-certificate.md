# Sign the macOS app with one self-signed certificate

Status: Accepted

## Context

The macOS app was signed ad hoc. An ad-hoc signature names nothing but the
build's own hash, so to macOS every build was a different app: it forgot what
the user had let the previous one do — screen recording, automation,
accessibility, access to folders — and asked again after each install. Adroit
has no Apple Developer ID.

## Decision

Every build of the macOS app is signed by the same self-signed certificate,
"Adroit Release", when the keychain has it, and ad hoc when it does not.

## Alternatives considered

- An Apple Developer ID. It would also let Gatekeeper open the app without the
  user's approval, but it costs a paid membership that the project does not
  hold.
- Stay ad hoc. Every install would keep asking the user again.
- Refuse to build without the certificate. A build on a Mac that never had it
  would fail for a permission it may never need.

## Consequences

- A rebuild keeps what the user let the previous one do.
- Gatekeeper still does not trust the app.
- Losing or replacing the certificate makes the user grant the app its
  permissions once more, so the certificate is kept outside the keychain too.
