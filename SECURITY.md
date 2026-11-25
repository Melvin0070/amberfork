# Security

## Reporting a vulnerability

Please report it privately through GitHub's
[private vulnerability reporting](https://github.com/Melvin0070/amberfork/security/advisories/new)
rather than a public issue. You should get a reply within a week. This is a solo-maintained
project, so there is no bounty and no SLA beyond that — but reports are read and fixes are
credited.

Only the latest released version is supported.

## What amberfork does that is security-relevant

amberfork is a local tool. It has no account, no telemetry, and makes no network call of its own
except where you point it. The parts worth scrutinising:

- **`record`, `replay` and `diff --verify` run a loopback proxy.** They bind `127.0.0.1` on an
  OS-chosen port and your agent sends its provider traffic — including its API key — through it.
  The proxy never listens on a non-loopback address. `record` and `replay --upstream` forward
  requests to the upstream you name on the command line and nowhere else.
- **Credentials are not written to cassettes.** Request and response headers are filtered through
  an allowlist before anything is stored, so `Authorization` and provider key headers are dropped.
- **Cassettes contain full request and response bodies, unredacted.** Prompts, tool results and
  model output are stored as sent. A cassette can therefore hold anything your agent handled:
  customer data, file contents, secrets a tool returned. `record` prints a warning every time.
  Treat a cassette like the data it was recorded from, and read
  [the privacy contract](docs/cassette-format.md#privacy-contract) before committing or sharing one.
- **`serve` binds `127.0.0.1` only** and serves a bundle embedded in the binary; it loads nothing
  from a CDN.
- **`record` and `replay` execute the command you give them** after `--`. They add no sandbox.

## Out of scope

Findings that require an attacker who already controls your shell, your trace files or your agent's
command line are not vulnerabilities in amberfork.
