# Security policy

## Reporting a vulnerability

**Do not open a public issue.**

Use GitHub's private vulnerability reporting:
[Report a vulnerability](https://github.com/phillip-simons/alpaca-sdk/security/advisories/new).

This is maintained by one person in their own time. Expect an acknowledgement
within a week. If you have had no reply in two weeks, assume it was missed and
send a reminder.

## Supported versions

The most recent published version only. This crate is pre-1.0; there are no
maintained release branches, and a fix ships in the next version rather than as
a backport.

## Scope

This is an unofficial API client. Worth reporting:

- **Credential leakage** — keys reaching a log, an error message, a panic
  payload, or a host they were not meant for. `BrokerClient`'s document-download
  path deliberately follows a redirect to presigned storage and relies on
  `reqwest` shedding credentials when one crosses hosts; a case where that does
  not hold is in scope.
- **TLS or certificate verification** being weakened or bypassed.
- **A dependency advisory** that `cargo-deny` does not already flag.
- **Deserialization of an untrusted payload** causing a panic or unbounded
  memory use. This crate forbids `unsafe`, so memory unsafety is not expected —
  but a panic on a hostile response is still a denial of service for a caller.

Not in scope, and better sent to Alpaca:

- Vulnerabilities in the Alpaca API itself, or in your account's configuration.
- Anything requiring an attacker to already hold your API credentials.

## What this crate does with your credentials

They are held in memory in `Credentials`, sent as `APCA-*` headers, a basic
`Authorization` header, or an OAuth bearer token, and are never written to disk
or logged. `RestClient` refuses redirects by default so that credentials cannot
follow one to another host.
