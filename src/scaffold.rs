//! Body scaffolds, one per kind.
//!
//! They are embedded in the binary and must never live as template files in the
//! vault: a template carries prose *and* a restatement of the schema, and the
//! restatement is a second copy.

use crate::kind::Kind;

/// The sections a kind's body starts with, already separated by blank lines.
#[must_use]
pub fn sections(kind: Kind) -> &'static str {
    match kind {
        Kind::Project => "## Purpose\n\n## Boundaries\n\n## Notes\n",
        Kind::Memory => "## Summary\n\n## Details\n",
        Kind::Plan => "## Goal\n\n## Steps\n\n- [ ] \n",
        Kind::Adr => "## Context\n\n## Decision\n\n## Considered options\n\n## Consequences\n",
        Kind::Context => {
            "## Summary\n\n## Language\n\n## Relationships\n\n## Example dialogue\n\n## Flagged ambiguities\n"
        }
    }
}

/// The body `new` writes for a kind: a blank line, the title heading, then the
/// kind's sections. The tool never touches body bytes again.
#[must_use]
pub fn body(kind: Kind, title: &str) -> String {
    format!("\n# {title}\n\n{}", sections(kind))
}

/// What a kind is for, in one line. `mnemex schema` prints this beside the
/// fields; the fuller account of each kind lives in a governed context note.
#[must_use]
pub fn purpose(kind: Kind) -> &'static str {
    match kind {
        Kind::Project => {
            "A directory and repository of work; the entrypoint a session resolves to."
        }
        Kind::Memory => {
            "Durable knowledge for one project that the repository, its git log, and its tracker cannot supply."
        }
        Kind::Plan => "A body of steps for one project; `open` until set `done`.",
        Kind::Adr => "A decision that constrains future work on one project.",
        Kind::Context => "The vocabulary and relationships of one project's bounded area.",
    }
}
