# Security

## Supported versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |

## Reporting a vulnerability

Report vulnerabilities through GitHub private vulnerability reporting:

https://github.com/mhalder/mnemex/security/advisories/new

Do not open a public issue for a security report.

## What matters here

mnemex reads and writes one governed markdown vault, so the
security-relevant surface is:

- The single-writer contract: authoring verbs take no lock, so two authoring
  verbs must not run against one vault at once. Nothing enforces this but the
  caller.
- Path handling under `MEMEX_VAULT`: a written file is judged against the one
  vault, and a note that is a symlink is written through to its target.
- The hook envelope: `mnemex hook` reads a tool-event payload on stdin and must
  never fail the write it reports on, so a malformed payload is handled, not
  trusted.
- No network at runtime: the binary never talks to the network; the only
  subprocess it runs is `git log` for `brief`'s recency ranking.
