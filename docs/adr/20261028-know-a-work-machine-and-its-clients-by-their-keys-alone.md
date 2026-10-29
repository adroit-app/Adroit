# Know a work machine and its clients by their keys alone

Status: Accepted

## Context

A work machine reached through its gateway needs an authenticated channel of
adroit's own. On a home or office network its address changes with
DHCP and the network it is on, and nothing beyond the two machines can vouch
for either of them.

## Decision

Each side keeps one key and presents it as a raw public key in TLS 1.3. Each
trusts the other by the fingerprint of that key, recorded when they paired, and
by nothing else: not a certificate authority, a name or an address.

## Alternatives considered

- A private certificate authority on the work machine that signs both sides.
  The fingerprints would still have to be recorded to tell a revoked or unknown
  key from a signed one, so the authority would add issuing, renewal and its
  own expiry without deciding anything.
- Bind trust to the work machine's name or address. An address change would
  look like another machine, or another machine at the old address would look
  like this one.
- Passwords or long-lived tokens. Whoever copies one can use it from
  anywhere, and both have to be stored and sent.

## Consequences

- A work machine stays the same machine when its address changes, and a
  different machine at a known address is refused.
- No certificate expires; replacing a key means pairing again.
