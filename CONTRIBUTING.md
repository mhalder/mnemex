# Contributing

For what the tool is and how to use it, see the [README](README.md). The schema
is not described here or anywhere else in prose: `mnemex schema` prints it, and
[`src/spec.rs`](src/spec.rs) is it.

## Prerequisites

Beyond a stable Rust toolchain, a few tools live outside the repo. Each gate
stage checks only the tools it needs and names what is missing:

| Stage | Needs |
| --- | --- |
| `fmt`, `clippy`, `doc`, `test` | `cargo` |
| `msrv` | `cargo`, `rustup`; the gate installs the declared toolchain when it is missing |
| `node` | Node 24 or newer, `npm`, `npx` |
| `deny` | `cargo install cargo-deny` |
| `coverage` | `cargo`, `cargo install cargo-llvm-cov` |

Reviewing snapshots: `cargo install cargo-insta` (optional;
`INSTA_UPDATE=always cargo test` works without it).

## The gate

One script runs every gate, in the order that fails fastest. Every gate must
pass before a commit:

```
./scripts/gate.sh
```

CI runs one job per stage, each `./scripts/gate.sh <stage>`. The stages are
`fmt`, `clippy`, `doc`, `msrv`, `test`, `node`, `deny`, `coverage`; no argument
means `all`, which runs them in that order. Each stage checks only its own
prerequisites and refuses a missing tool with exit 2 before running anything;
`all` checks every tool before any stage runs.

| Step | What it catches |
| --- | --- |
| `cargo fmt --check` | formatting |
| `cargo clippy --all-targets -D warnings` | the lint set pinned in `Cargo.toml` (`pedantic` denied) |
| `cargo doc --no-deps`, rustdoc warnings denied | a dangling doc link, which no other gate builds |
| `cargo check` on the declared `rust-version` | a dependency or language feature newer than the MSRV |
| `cargo test` | everything below |
| `npm ci`, `tsc`, `node --test` | the Pi extension and the Claude hook, against the Pi types pinned in `package.json` |
| `cargo deny check` | a RustSec advisory, a yanked crate, a licence outside `deny.toml`'s list, an unknown source |
| coverage | a line floor of 95%, in total **and in every file** |

The total alone would hide a badly covered file behind well covered ones, so
the floor applies per file too. `COVERAGE_FLOOR` may raise the floor, never
lower it.

Every cargo build runs with `--locked`, so a manifest change the committed
`Cargo.lock` does not match fails the gate instead of being resolved quietly
mid-run.

`tests/gate_script.rs` runs the script against stub commands to hold its own
guards and step order.

The gate takes minutes; run it under `tmux-run` or a spare pane rather than in
a blocking call.

## Test discipline

Every change begins with a test that fails for the reason the requirement
names. Coverage is a floor under that, never a substitute for it.

`tests/` drives the library directly for most behaviour and the built binary
only where the binary is the subject: exit codes, help, the hook envelope,
completion. The files are named for what they hold; the ones worth knowing
before an edit:

| File | Holds |
| --- | --- |
| `surface_snapshots.rs` | every verb's `--help`, both `schema` renderings, and both `check` reports, as `insta` snapshots |
| `fixture_rules.rs` | one `good/` and one `bad/` vault per rule, under `tests/fixtures/<code>/` |
| `check_messages.rs` | every rule's message and severity |
| `spec_invariants.rs`, `schema_driven_kinds.rs` | the schema table's shape, and tests generated from it for every kind |
| `skill_budget.rs` | the skill's byte ceiling, and that neither it nor the README restates the schema |
| `frontmatter_roundtrip.rs` | parse, emit, parse is the identity (`proptest`) |
| `complete.rs` | shell completion, down to the binary answering `COMPLETE=bash`, `COMPLETE=zsh`, and `COMPLETE=fish` |

A surface change shows up as a snapshot diff. Review it with
`cargo insta review` and accept it only when the new text is what the skill
should now describe: the snapshots exist so that a renamed verb cannot leave
the skill quietly lying.

## Layout

```
src/spec.rs         the one schema table
src/kind.rs         the five kinds and their folders
src/cli.rs          the clap surface; the derive structs are the surface
src/complete.rs     what a Tab press offers
src/main.rs         printing and exit codes only
src/check.rs        the rules; src/diagnostic.rs their codes
src/authoring.rs    new, adopt, set, rename, delete
src/query.rs        resolve, list, show, project
src/frontmatter.rs  the parse/span/emit engine
skills/mnemex/      the skill
extensions/          the Pi extension
hooks/              the Claude hook
scripts/gate.sh     every gate
```

The library holds every decision; the binary only parses, prints and picks an
exit code, so integration tests can drive the library without a process.

## Invariants worth knowing before you edit

- **The schema has one copy.** `check` walks `spec`, `authoring` emits from it,
  `schema` prints it, and completion reads it. Anything that restates a kind,
  a field or an enum value — the skill, the README, a hook, a shell script —
  is a copy that drifts. `skill_budget.rs` enforces this for the prose.
- **Diagnostic codes are never reused.** A dropped rule leaves a hole; the
  holes are listed in `src/diagnostic.rs`. A new rule takes an unused code in
  its family's band.
- **Single writer.** Authoring verbs take no lock. A note is written to a
  sibling temp file and renamed into place, but two authoring verbs against one
  vault can still lose an update.
- **A note's id is its filename stem.** Nothing in frontmatter records it.

## Adding a rule

1. Add a `Code` variant in `src/diagnostic.rs` with an unused code, and add it
   to `Code::ALL`.
2. Emit it from `src/check.rs`.
3. Add `tests/fixtures/<code>/good/` and `bad/`, each a small vault. The harness
   fails on a rule without both. A fixture that needs a checkout commits
   `dot-git`, not `.git`: Git refuses to track any path component named `.git`,
   so the harness copies the vault to a temp directory and renames it.
4. Pin its message and severity in `tests/check_messages.rs`.

## Adding a verb or an argument

1. Add it to `src/cli.rs`. A new verb also goes into `every_verbs_help` in
   `tests/surface_snapshots.rs`; accept the new snapshot.
2. An argument that takes a note id, a kind, a status, a field or a tag gets a
   completer from `src/complete.rs` (`add = ArgValueCompleter::new(..)` or
   `ArgValueCandidates`); a path gets a `value_hint`. Without one, Tab offers
   nothing for that argument.
3. Update `skills/mnemex/SKILL.md` if the skill should route to it, within its ceiling.

## Dependencies

`Cargo.lock` is committed. Direct dependencies take a semver range, except two
pinned exact:

- `saphyr` and `saphyr-parser`, which are pre-1.0 and under the frontmatter
  engine's spans. Bump them deliberately; `frontmatter.rs` and
  `frontmatter_roundtrip.rs` are the check.
- `clap_complete`, whose `unstable-dynamic` feature may change between minor
  releases.

`cargo update` resolves only versions compatible with the declared
`rust-version`, and says which it held back. Raising the MSRV is a decision,
not a side effect of an update.

`package.json` pins the hook toolchain exact. `@types/node` tracks the Node
major the hooks run on, not the newest `@types/node`.

## Releasing

Releases follow [Semantic Versioning](https://semver.org/), driven by
conventional commits and [release-plz](https://release-plz.dev/). The version
lives in `Cargo.toml` and nowhere else.

The commit type decides the bump, and whether a release happens at all:

| Commit | Release |
| --- | --- |
| `feat:` | minor |
| `fix:` | patch |
| `perf:`, `refactor:` | patch, in "Changed" |
| `revert:` | patch, in "Removed" |
| any type with `!` | the breaking change, whatever the type |
| `build:`, `chore:`, `ci:`, `docs:`, `style:`, `test:` | no release, no changelog entry |

A breaking change is marked with `!` on the commit type; a `BREAKING CHANGE:`
footer alone on an otherwise skipped type (`docs:`, `chore:`, ...) does not
open a release PR.

Nobody bumps the version by hand. A merged commit of a releasing type makes
release-plz open or update a release PR that bumps `Cargo.toml` and
`Cargo.lock` and writes the `CHANGELOG.md` section. A release is a merged
release PR: `release_always = false`, so merging only `ci:` or `docs:` commits
releases nothing. Merging the release PR tags `vX.Y.Z`, publishes to crates.io
through Trusted Publishing, and starts the binary matrix.

A deliberate bump that conventional commits would not produce is a deliberate
override: edit the release PR's version (with `release-plz set-version` or by
hand) before merging, not a hand-edited `Cargo.toml` on main.

The release PR is merged with the repository's admin bypass: a PR opened with
the default `GITHUB_TOKEN` runs no CI, so the branch-protection check would
otherwise block the merge.

Do not edit `CHANGELOG.md` or `version` yourself; the release PR owns both.

## License

By contributing, you agree that your contribution is dual-licensed under MIT OR Apache-2.0, unless you state otherwise.
