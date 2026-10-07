//! Human and JSON renderings.
//!
//! One version constant is shared by every `--json` envelope, and every outcome
//! is in the envelope — including the absences — so a consumer reads the payload
//! rather than inferring from an exit code.

use core::fmt::Write as _;

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Severity};
use crate::frontmatter::{Field, Position, Value};

/// The version every `--json` envelope carries.
pub const VERSION: u32 = 1;

/// How many of each severity a run produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Errors. Only these affect the exit code.
    pub error: usize,
    /// Warnings.
    pub warning: usize,
    /// Infos.
    pub info: usize,
}

impl Summary {
    /// Count `diagnostics`.
    #[must_use]
    pub fn of(diagnostics: &[Diagnostic]) -> Self {
        let mut s = Summary::default();
        for d in diagnostics {
            match d.severity {
                Severity::Error => s.error += 1,
                Severity::Warning => s.warning += 1,
                Severity::Info => s.info += 1,
            }
        }
        s
    }

    /// Whether any error was found. Only errors affect the exit code.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.error > 0
    }

    /// Whether nothing at all was found.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.error == 0 && self.warning == 0 && self.info == 0
    }

    /// The summary line, or `None` when nothing was found.
    ///
    /// Counts are pluralised, `info` never is, and zero counts are omitted.
    #[must_use]
    pub fn line(&self) -> Option<String> {
        let mut parts = vec![];
        if self.error > 0 {
            parts.push(format!("{} {}", self.error, plural(self.error, "error")));
        }
        if self.warning > 0 {
            parts.push(format!(
                "{} {}",
                self.warning,
                plural(self.warning, "warning")
            ));
        }
        if self.info > 0 {
            parts.push(format!("{} info", self.info));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

/// One diagnostic line: `<path>:<line>:<col>: <severity>[<code>]: <message>`.
/// A spanless diagnostic omits the position.
#[must_use]
pub fn line(d: &Diagnostic) -> String {
    let mut out = d.path.display().to_string();
    if let Some(Position { line, column }) = d.span {
        let _ = write!(out, ":{line}:{column}");
    }
    let _ = write!(out, ": {}[{}]: {}", d.severity, d.code, d.message);
    out
}

/// The human report: `clean` when there is nothing to say, otherwise one line
/// per diagnostic, a blank line, then the summary. This is the shape an
/// authoring verb reports its note's diagnostics with.
#[must_use]
pub fn human(diagnostics: &[Diagnostic]) -> String {
    let summary = Summary::of(diagnostics);
    let Some(tail) = summary.line() else {
        return "clean\n".to_owned();
    };
    let mut out = String::new();
    for d in diagnostics {
        out.push_str(&line(d));
        out.push('\n');
    }
    let _ = write!(out, "\n{tail}\n");
    out
}

/// The `check` human report: findings, then exactly one trailer line.
///
/// `clean, <n> notes` when nothing was found, else `<e> errors, <w> warnings in
/// <n> notes`, with singular forms when the count is 1.
#[must_use]
pub fn check_human(diagnostics: &[Diagnostic], notes: usize) -> String {
    let summary = Summary::of(diagnostics);
    if summary.is_clean() {
        return format!("clean, {} {}\n", notes, plural(notes, "note"));
    }
    let mut out = String::new();
    for d in diagnostics {
        out.push_str(&line(d));
        out.push('\n');
    }
    let _ = write!(
        out,
        "\n{} in {} {}\n",
        summary.line().unwrap_or_default(),
        notes,
        plural(notes, "note")
    );
    out
}

#[derive(Serialize)]
struct SpanJson {
    line: usize,
    column: usize,
}

#[derive(Serialize)]
struct DiagnosticJson<'a> {
    path: String,
    code: &'a str,
    severity: &'a str,
    message: &'a str,
    span: Option<SpanJson>,
}

#[derive(Serialize)]
struct CheckJson<'a> {
    version: u32,
    /// How many notes were checked, so a clean result over no notes is not
    /// mistaken for a clean vault.
    checked: usize,
    diagnostics: Vec<DiagnosticJson<'a>>,
    summary: Summary,
}

/// Pretty-print an envelope with the trailing newline every `--json` payload has.
pub(crate) fn envelope<T: Serialize>(value: &T) -> String {
    // Every envelope in this crate is a plain tree of strings, numbers and
    // sequences, which `serde_json` cannot fail to render.
    let mut out = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_owned());
    out.push('\n');
    out
}

/// The `check --json` envelope.
#[must_use]
pub fn check_json(diagnostics: &[Diagnostic], checked: usize) -> String {
    envelope(&CheckJson {
        version: VERSION,
        checked,
        diagnostics: diagnostics
            .iter()
            .map(|d| DiagnosticJson {
                path: d.path.display().to_string(),
                code: d.code.as_str(),
                severity: d.severity.as_str(),
                message: &d.message,
                span: d.span.map(|p| SpanJson {
                    line: p.line,
                    column: p.column,
                }),
            })
            .collect(),
        summary: Summary::of(diagnostics),
    })
}

// --- query renderings ----------------------------------------

use std::path::Path;

use crate::authoring::Outcome;
use crate::query::{NoteObject, ProjectAnswer, ShowAnswer};

/// A path as a target line prints it: vault-relative where it can be.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// A scalar `status` field, when the note has one.
fn status_of(n: &NoteObject) -> Option<&str> {
    n.fields
        .as_ref()?
        .iter()
        .find(|f| f.key == "status")
        .and_then(|f| f.value.as_scalar())
}

/// One note, as the human form lines it up: `<kind>/<id> — <title>`, with
/// ` (<status>)` when the kind has one.
fn note_line(n: &NoteObject) -> String {
    let mut line = format!("{}/{} — {}", n.kind.name(), n.id, n.title);
    if let Some(status) = status_of(n) {
        let _ = write!(line, " ({status})");
    }
    line
}

/// What a verb did, its note's path, and the report if the note is not clean.
#[must_use]
pub fn outcome_human(outcome: &Outcome) -> String {
    let mut out = format!("{}\n{}\n", outcome.headline, outcome.path.display());
    for note in &outcome.notes {
        let _ = writeln!(out, "{note}");
    }
    if !outcome.diagnostics.is_empty() {
        let _ = write!(out, "\n{}", human(&outcome.diagnostics));
    }
    out
}

/// `resolve`, human: `kind: title` then the path.
#[must_use]
pub fn resolve_human(r: &NoteObject) -> String {
    format!("{}: {}\n{}\n", r.kind.name(), r.title, r.path.display())
}

/// The frontmatter fields of a note, as an ordered JSON object: scalars as
/// strings, lists as arrays of strings.
pub(crate) fn fields_json(fields: &[Field]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for f in fields {
        let value = match &f.value {
            Value::Scalar(s) => serde_json::Value::String(s.clone()),
            Value::List(items) => serde_json::Value::Array(
                items
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            ),
            Value::Unsupported => serde_json::Value::Null,
        };
        map.insert(f.key.clone(), value);
    }
    serde_json::Value::Object(map)
}

#[derive(Serialize)]
pub(crate) struct NoteJson<'a> {
    id: &'a str,
    kind: &'a str,
    path: String,
    title: &'a str,
    fields: Option<serde_json::Value>,
}

impl<'a> From<&'a NoteObject> for NoteJson<'a> {
    fn from(n: &'a NoteObject) -> Self {
        NoteJson {
            id: &n.id,
            kind: n.kind.name(),
            path: n.path.display().to_string(),
            title: &n.title,
            fields: n.fields.as_ref().map(|fs| fields_json(fs)),
        }
    }
}

#[derive(Serialize)]
struct ResolveJson<'a> {
    version: u32,
    note: Option<NoteJson<'a>>,
}

/// `resolve --json`.
#[must_use]
pub fn resolve_json(r: Option<&NoteObject>) -> String {
    envelope(&ResolveJson {
        version: VERSION,
        note: r.map(Into::into),
    })
}

/// `list`, human: one note per line, as its vault-relative path and title.
#[must_use]
pub fn list_human(root: &Path, notes: &[NoteObject]) -> String {
    let mut out = String::new();
    for n in notes {
        let _ = writeln!(out, "{} — {}", relative(root, &n.path), n.title);
    }
    out
}

#[derive(Serialize)]
struct ListJson<'a> {
    version: u32,
    notes: Vec<NoteJson<'a>>,
}

/// `list --json`.
#[must_use]
pub fn list_json(notes: &[NoteObject]) -> String {
    envelope(&ListJson {
        version: VERSION,
        notes: notes.iter().map(Into::into).collect(),
    })
}

/// `project`, human: the winning project, then what it matched on.
#[must_use]
pub fn project_human(a: &ProjectAnswer) -> String {
    let Some(note) = &a.note else {
        return String::new();
    };
    let mut out = format!("{} — {}\n", note.id, note.title);
    match a.via {
        Some("repo") => {
            let _ = writeln!(out, "via repo {}", a.detail.as_deref().unwrap_or_default());
        }
        Some("path") => {
            let _ = writeln!(out, "via path {}", a.detail.as_deref().unwrap_or_default());
        }
        _ => {}
    }
    out
}

#[derive(Serialize)]
struct ProjectJson<'a> {
    version: u32,
    dir: String,
    via: Option<&'a str>,
    note: Option<NoteJson<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    candidates: Vec<NoteJson<'a>>,
}

/// `project --json`.
#[must_use]
pub fn project_json(a: &ProjectAnswer) -> String {
    envelope(&ProjectJson {
        version: VERSION,
        dir: a.dir.display().to_string(),
        via: a.via,
        note: a.note.as_ref().map(Into::into),
        candidates: a.candidates.iter().map(Into::into).collect(),
    })
}

#[derive(Serialize)]
struct ProjectRefJson<'a> {
    id: &'a str,
    note: Option<NoteJson<'a>>,
}

#[derive(Serialize)]
struct SpokesJson<'a> {
    plans: Vec<NoteJson<'a>>,
    adrs: Vec<NoteJson<'a>>,
    memories: Vec<NoteJson<'a>>,
    contexts: Vec<NoteJson<'a>>,
}

/// `show`, human: the note, its project, then the project's spokes by kind.
#[must_use]
pub fn show_human(a: &ShowAnswer) -> String {
    let mut out = format!("{}\n", note_line(&a.note));
    match &a.project {
        Some(p) => {
            let _ = writeln!(out, "project: {}", note_line(p));
        }
        None if a.project_id.is_empty() => out.push_str("project: (none)\n"),
        None => {
            let _ = writeln!(out, "project: {} (no such note)", a.project_id);
        }
    }
    for (header, notes) in [
        ("plans", &a.spokes.plans),
        ("adrs", &a.spokes.adrs),
        ("memories", &a.spokes.memories),
        ("contexts", &a.spokes.contexts),
    ] {
        if notes.is_empty() {
            continue;
        }
        let _ = writeln!(out, "\n{header}:");
        for n in notes {
            let _ = writeln!(out, "  {}", note_line(n));
        }
    }
    out
}

#[derive(Serialize)]
struct ShowJson<'a> {
    version: u32,
    note: NoteJson<'a>,
    project: ProjectRefJson<'a>,
    spokes: SpokesJson<'a>,
}

/// `show --json`.
#[must_use]
pub fn show_json(a: &ShowAnswer) -> String {
    envelope(&ShowJson {
        version: VERSION,
        note: (&a.note).into(),
        project: ProjectRefJson {
            id: &a.project_id,
            note: a.project.as_ref().map(Into::into),
        },
        spokes: SpokesJson {
            plans: a.spokes.plans.iter().map(Into::into).collect(),
            adrs: a.spokes.adrs.iter().map(Into::into).collect(),
            memories: a.spokes.memories.iter().map(Into::into).collect(),
            contexts: a.spokes.contexts.iter().map(Into::into).collect(),
        },
    })
}
