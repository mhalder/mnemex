---
name: mnemex
description: Record and query notes in the governed markdown vault with mnemex. Use for every write into the vault, and to navigate it.
---

# mnemex

`mnemex schema` is the schema — kinds, fields, closed enums, defaults, and body
scaffolds. `<verb> --help` is the surface. Never restate either, and never grep
the vault: `mnemex show`, `mnemex list --project`, `mnemex list --tag` (notes
carrying every given tag), `mnemex project --for`, and `mnemex brief --for`
answer from outside. `mnemex resolve <id>`
finds one note by id and prints its path; read the body from there.

## Workflow

1. Read the scaffold for the kind from `mnemex schema`, then
   `mnemex new <kind> <title> --project <id>`.
2. Edit the body below the frontmatter fence with the editing tools.
3. Run `mnemex check` before finishing. `mnemex set <plan> status done`
   closes a plan.

## What belongs in a note

Admit a note only when one of three rules says so:

- **the repo knows it** — nothing a fresh clone plus `git log` can tell goes
  into a note.
- **the README says it** — a project body holds purpose and boundaries only.
- **the ADR decides it** — if violating it later would be a bug, it is an ADR;
  otherwise it is text in a plan step.

A replaced ADR is set `deprecated`; the replacing ADR links it in its Context.

## Conventions

- Titles under eight words.
- A note that records a tracker item carries that item's URL as a ref — `--ref`
  on the mint, `mnemex set <id> refs <url>...` later. Paste any spelling; the
  tool normalises it.
- `mnemex set` on a list field replaces the whole list: give it every value,
  old and new.

## Hard rules

- Never hand-edit frontmatter.
- Never write into the vault from the shell.
- Never remove a field to silence a diagnostic.
- Authoring verbs one at a time.

## Updating

This skill is one file in the tool's repository. The skills CLI installs it
(`skills add mhalder/mnemex -g -a pi -a claude-code -y`) and `skills update`
refreshes it from the repository's default branch.

## Exit codes

`0` clean; `1` errors present or a query that found nothing; `2` a refused
verb, usage error, or I/O failure. A refused verb names the alternative in its
message.
