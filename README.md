# `mnemex`

[![crates.io](https://img.shields.io/crates/v/mnemex.svg)](https://crates.io/crates/mnemex)
[![CI](https://github.com/mhalder/mnemex/actions/workflows/ci.yml/badge.svg)](https://github.com/mhalder/mnemex/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](https://github.com/mhalder/mnemex#license)

A single binary that authors and validates notes in a governed markdown vault.

Two invariants define it:

1. **One executable schema.** The schema exists in exactly one place — inside
   the binary — and is printable on demand: `mnemex schema`. Nothing else
   restates any part of it. A schema stated twice is wrong in at least one of
   the two.
2. **Validation on the write.** A violation reaches the writer as a coded,
   spanned diagnostic within milliseconds of the write that caused it.

There is one vault: the directory `$MNEMEX_VAULT` names, or `~/mnemex/vault` when it is
unset.

## Install

```sh
cargo install mnemex
```

To install the unreleased `main` instead:

```sh
cargo install --git https://github.com/mhalder/mnemex
```

Prebuilt binaries are published on [GitHub Releases](https://github.com/mhalder/mnemex/releases) for:

- Linux x86_64 and aarch64, static musl builds
- macOS x86_64 and aarch64

Shell completion is answered by the binary itself on each Tab press, so ids
come from the vault as it is now. A note id completes from any part of its id
or title: `mnemex show lab<Tab>` becomes `202609231653-home-lab`.
`mnemex completion <bash|zsh|fish|elvish|powershell>` prints the activation
snippet to source on shell startup:

```
mnemex completion fish
```

## The surface

```
mnemex new <kind> <title> [--project <id>] [--status <s>] [--tag <t>]... [--ref <url>]... [--path <dir>] [--repo <remote>]
mnemex set <note> <field> (<value>... | --clear)
mnemex rename <note> <new title> [--dry-run]
mnemex delete <note>
mnemex adopt <file> --kind <k> [--project <id>] [--status <s>] [--tag <t>]... [--ref <url>]...

mnemex skill [--install [dir]] [--check [dir]]
mnemex extension <pi|claude> [--install [dir]] [--check [dir]]
mnemex completion <bash|zsh|fish|elvish|powershell>

mnemex check [--path <file>] [--json]
mnemex schema [--json]
mnemex hook

mnemex resolve <id> [--json]
mnemex list [--kind <k>] [--project <id>] [--json]
mnemex project --for <dir> [--json]
mnemex show <id> [--json]
mnemex brief --for <dir> [--budget <bytes>] [--json]
```

Every verb that takes a note takes its full id: the twelve-digit stamp, a
hyphen, and the slug, as in `202609231653-home-lab`. A bare slug is not an
id and resolves nothing.

Exit codes: `0` clean, `1` errors present (`check`) or a query that found
nothing or could not answer fully, `2` usage error, I/O failure, or a refused
verb. A refused verb names the alternative in its message.

`mnemex schema` is the schema. This README does not restate it, and neither
should anything else. It prints each kind's folder, fields, closed enums,
one-line purpose, and the body scaffold `new` writes.

## The project is the entrypoint; the spoke owns the edge

Every non-project note carries a `project` field naming its project. The
project note carries no lists of its own. Obsidian shows the edge from both
ends through backlinks; the tool answers "what belongs to this project" by
scanning frontmatter — `list --project <id>` returns a project's spokes,
`show <id>` returns a note's project and that project's spokes grouped by kind.

A project note carries two facts about the work it governs:

- **the directory** (`path`) — where the work happens _on this machine_. A
  relative spelling is taken from the working directory, and a path inside
  `$HOME` is stored `~/`-relative so it stays portable.
- **the repository** (`repo`) — _which_ repository the work is, which survives
  a second machine, a second clone, and a `mv`. It is stored canonically, as
  `host/owner/name`: every spelling of one remote is stored as the same value,
  so two clones of one repository compare equal.

Naming the directory fills the repository in, and says so:

```
$ mnemex new project "mnemex" --path ~/src/mnemex
created 202609100010-mnemex
/home/x/mnemex/projects/202609100010-mnemex.md
`repo` is `github.com/mhalder/mnemex`, from the checkout at `/home/x/src/mnemex`
```

The remote is read from `.git/config`, **never from a `git` subprocess**. The
one exception is `brief`, which runs a single `git log` over the whole vault to
rank notes by recency. A stored `repo` is never overwritten by a derived one;
a disagreement is a warning, and a checkout path with no `repo` is one too. The
check matches `repo` against **every** remote of the checkout — a fork kept
beside an upstream `origin`, a vault whose remotes name the layers it serves —
while the derivation names `origin` alone: which of several remotes a project
*is*, only the writer knows.

## Which project owns a directory

`mnemex project --for <dir>` answers by two tests. First the **repo match**:
walk up to the first ancestor holding `.git`, compute its canonical `origin`,
and collect the projects whose `repo` equals it. Then the **path match**: every
project whose `path` contains the directory, with the longest resolved path
winning. A path match wins over a repo match (a monorepo may hold several
projects); more than one candidate at the winning level is ambiguity, reported
with every candidate id. The scan touches `projects/` only, so it is flat in
vault size and safe to run from a status line every few hundred milliseconds.

## `brief` is the session start

`mnemex brief --for <dir>` prints the project body verbatim, the open plans
with their step counts, memories newest first, ADRs, contexts, and a `Next`
block telling the session which plan to read first and which done plans to
close. Context bodies are cut to an output budget (default 16384 bytes).

## Using with Pi

The skill and the write-check extension ship inside the binary and install
with it:

```
$ mnemex skill --install
created ~/.agents/skills/mnemex/SKILL.md
$ mnemex extension pi --install
created ~/.pi/agent/extensions/mnemex.ts
```

`skill` writes the agent skill; `extension pi` writes the Pi extension. Each
defaults to its directory above — `skill` to `~/.agents/skills/mnemex`,
`extension pi` to `$PI_CODING_AGENT_DIR/extensions` or
`~/.pi/agent/extensions` — and a directory argument overrides it.
`--check` replaces `--install` to byte-compare the installed file and exit 1
when it is missing or stale. Neither command touches the vault.

## Using with Claude Code

`mnemex extension claude --install` writes the same write check as one command
hook, run by Node directly, to `~/.claude/hooks/mnemex.ts` (a directory
argument overrides it). It prints the `settings.json` snippet to register the
hook for `PostToolUse` (matcher `Write|Edit`) and `Stop`; it does not edit
`settings.json` itself.

`hook` already answers in Claude's hook envelope, so a write that fails the
check reaches the model as a block reason.

Both integrations sweep the whole vault with `check` at the end of every turn,
not only after a write through the editing tools, so a shell write, an editor,
or another session is caught too. Findings are shown only when they differ
from what that session last showed, and a vault that turns clean says so once.
Pi keeps the last shown sweep in memory; Claude runs each hook as its own
process, so it keeps a digest per session under `$XDG_RUNTIME_DIR` (or the
temp directory) and shows findings as a `systemMessage`.

The binary embeds all three of these files. A shell status line can use
`mnemex project --for "$PWD" --json`. The `hook` subcommand is for Pi's
extension and other tool-event integrations; agents should not invoke it by
hand. It judges a written file against the one vault: only a `.md` file
directly inside one of that vault's governed folders is checked, so a
repository's own `plans/` folder, or an archive inside the vault, is never
mistaken for the vault. `check --path` uses the same rule. A whole-vault
`check` also derives each field's Obsidian property type from the schema and
fails when `.obsidian/types.json` disagrees; `check --path` checks the one
note and not the file.

## One writer

`mnemex` is a single-writer tool. Authoring verbs (`new`, `adopt`, `set`,
`rename`, `delete`) write the vault; `check` and `hook` only read it. A note is
written by rendering to a sibling temp file and renaming it into place, so a
reader never sees a half-written note. A note that is a symlink is written
through to its target, and a note keeps its file permissions across the write.
Still, two authoring verbs must not run against one vault at once: the
read-modify-write each verb performs can still lose the other's update. The
tool takes no lock; the single-writer contract is the caller's. Readers
(`check`, the hook, the settle sweep) are safe beside each other and beside one
authoring verb. On write, the frontmatter block is re-emitted with `\n` line
endings; the body below the closing fence is preserved byte-for-byte, CRLF
included.

The block is rewritten, never patched. A YAML comment in it is dropped, and
every value comes back in the tool's own style: a `|` or `>` block scalar
becomes one double-quoted string with escapes, and a flow list such as
`[a, b]` becomes a block list. Anything meant to last belongs in the body.

## Developing

See [CONTRIBUTING.md](CONTRIBUTING.md): prerequisites, the gate and its stages,
the test layout, the invariants, and how to add a rule, a verb, or a
dependency.

## License

Licensed under MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).

Contributions are welcome; see [CONTRIBUTING.md](CONTRIBUTING.md). To report a
vulnerability, see [SECURITY.md](SECURITY.md).
