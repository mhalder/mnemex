//! Codes, severities and diagnostics.
//!
//! Codes are `MX` plus three zero-padded digits, banded by family so rules can
//! be added without renumbering. They are never reused or renumbered: a dropped
//! rule leaves a hole rather than closing it. The holes are `MX101`, `MX103`,
//! `MX106`, `MX107`, `MX109`, `MX201`, `MX203`, `MX205`, `MX206`, `MX300`,
//! `MX301`, `MX302`, `MX400`, and `MX402`, and none is reissued.
use std::path::PathBuf;

use crate::frontmatter::Position;

/// A rule code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Code {
    /// the block could not be parsed
    Mx001,
    /// a top-level key appears more than once
    Mx002,
    /// a value is neither scalar nor list-of-scalars
    Mx003,
    /// a required field is absent, or is an empty scalar
    Mx100,
    /// value outside a closed enum
    Mx102,
    /// wrong container
    Mx104,
    /// a field the schema does not know for this kind
    Mx105,
    /// a `git remote` field is not the canonical `host/owner/name`
    Mx108,
    /// a `refs` entry is not a normalised work-item URL
    Mx110,
    /// a link-shaped value is not exactly `[[id]]`
    Mx200,
    /// a frontmatter link names a note not in the vault
    Mx202,
    /// a link names a note of a kind its field does not point at
    Mx204,
    /// filename is not a valid id
    Mx401,
    /// `path` names a directory that is not there
    Mx403,
    /// `repo` and the remote of the directory `path` names disagree
    Mx404,
    /// `path` names a checkout but `repo` is absent
    Mx405,
    /// the same id names notes in more than one governed folder
    Mx406,
    /// `.obsidian/types.json` does not match the property types the schema derives
    Mx407,
}

impl Code {
    /// Every code, in numeric order.
    pub const ALL: [Code; 18] = [
        Code::Mx001,
        Code::Mx002,
        Code::Mx003,
        Code::Mx100,
        Code::Mx102,
        Code::Mx104,
        Code::Mx105,
        Code::Mx108,
        Code::Mx110,
        Code::Mx200,
        Code::Mx202,
        Code::Mx204,
        Code::Mx401,
        Code::Mx403,
        Code::Mx404,
        Code::Mx405,
        Code::Mx406,
        Code::Mx407,
    ];

    /// The code as it prints.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Code::Mx001 => "MX001",
            Code::Mx002 => "MX002",
            Code::Mx003 => "MX003",
            Code::Mx100 => "MX100",
            Code::Mx102 => "MX102",
            Code::Mx104 => "MX104",
            Code::Mx105 => "MX105",
            Code::Mx108 => "MX108",
            Code::Mx110 => "MX110",
            Code::Mx200 => "MX200",
            Code::Mx202 => "MX202",
            Code::Mx204 => "MX204",
            Code::Mx401 => "MX401",
            Code::Mx403 => "MX403",
            Code::Mx404 => "MX404",
            Code::Mx405 => "MX405",
            Code::Mx406 => "MX406",
            Code::Mx407 => "MX407",
        }
    }
}

/// How much a finding matters. Only errors affect the exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Blocks.
    Error,
    /// Worth knowing about.
    Warning,
    /// Said out loud, nothing more.
    Info,
}

impl Severity {
    /// The severity as it prints.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// One finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The note the finding is about.
    pub path: PathBuf,
    /// The rule.
    pub code: Code,
    /// How much it matters.
    pub severity: Severity,
    /// What it says.
    pub message: String,
    /// Where it applies, when a span is meaningful.
    pub span: Option<Position>,
}

impl core::fmt::Display for Code {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl core::fmt::Display for Severity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Diagnostic {
    /// Sort key within one note: by span, then code; spanless sorts first.
    #[must_use]
    pub fn sort_key(&self) -> (Option<(usize, usize)>, Code) {
        (self.span.map(|p| (p.line, p.column)), self.code)
    }
}
