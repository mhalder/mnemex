//! `mnemex follow-ups` — the bullets a plan leaves behind when it closes.
//!
//! A plan closes once its goal is met, follow-ups and all, so its
//! `## Follow-ups` bullets are work beyond the goal, minted as a plan only when
//! that work starts. `brief` lists the plans that are not done, so once a plan
//! closes its bullets drop out of every session. This verb is how they are read
//! afterwards: on demand, in full, and never injected into a session.

use std::path::Path;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::frontmatter::Document;
use crate::index::Index;
use crate::kind::Kind;
use crate::markdown::Fences;
use crate::query::{self, NoteObject};

/// The heading a plan's follow-ups live under.
const HEADING: &str = "## Follow-ups";

/// One follow-up bullet, as written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Item {
    /// Its 1-based position in the section, for display only: it is not a
    /// stable address.
    pub n: usize,
    /// Its text, as written: newline-separated, dedented, without the bullet
    /// marker and without end blank lines.
    pub text: String,
}

/// The follow-up items of a plan body.
///
/// The section runs from a prose `## Follow-ups` line to the next prose `## `
/// line or the end of the body. An item starts at a prose line at column zero
/// that is `-` alone or begins `- `. It ends at the next item, the end of the
/// section, or any other non-blank line at column zero, whether that line is
/// prose, a fence marker, or a line inside a fence. A blank line never ends an
/// item.
///
/// Every line from the bullet to the item's end is its text, kept as written:
/// the bullet marker comes off the first line, the further lines lose only the
/// indentation they share, and trailing whitespace is trimmed. Blank lines
/// inside the text stay; leading and trailing ones go. An item whose text is
/// empty is no item.
#[must_use]
pub fn items(body: &str) -> Vec<Item> {
    let mut fences = Fences::default();
    let mut found = vec![];
    let mut current: Option<Vec<&str>> = None;
    let mut in_section = false;
    for line in body.lines() {
        // Fences decide what can be a heading or a bullet, never what goes
        // into an item's text.
        let prose = fences.is_prose(line);
        if prose && line.trim_end() == HEADING {
            if in_section {
                break;
            }
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if prose && line.starts_with("## ") {
            break;
        }
        if prose && is_bullet(line) {
            close(&mut found, &mut current);
            current = Some(vec![line]);
            continue;
        }
        if !line.trim().is_empty() && !line.starts_with([' ', '\t']) {
            // Whatever it is, a non-blank line at column zero ends the item.
            close(&mut found, &mut current);
            continue;
        }
        // A blank line ends nothing, and an indented line is the item's own.
        if let Some(lines) = &mut current {
            lines.push(line);
        }
    }
    close(&mut found, &mut current);
    found
}

/// Whether a prose line starts a bullet: `-` alone or `- `.
fn is_bullet(line: &str) -> bool {
    line == "-" || line.starts_with("- ")
}

/// Finish the item under construction, numbering it by its position.
fn close(found: &mut Vec<Item>, current: &mut Option<Vec<&str>>) {
    let Some(lines) = current.take() else {
        return;
    };
    let text = item_text(&lines);
    if !text.is_empty() {
        found.push(Item {
            n: found.len() + 1,
            text,
        });
    }
}

/// A bullet's lines as text: the marker off the first line, the rest dedented
/// by the indentation they share, trailing whitespace trimmed, end blank lines
/// dropped, joined with newlines.
fn item_text(lines: &[&str]) -> String {
    let Some((first, rest)) = lines.split_first() else {
        return String::new();
    };
    let first = first
        .strip_prefix("- ")
        .or_else(|| first.strip_prefix('-'))
        .unwrap_or_default()
        .trim_end();
    // Every further line is blank or indented: anything else at column zero
    // ended the item.
    let indent = rest
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<&str> = vec![first];
    out.extend(rest.iter().map(|line| {
        let dedented = if line.len() >= indent {
            &line[indent..]
        } else {
            ""
        };
        dedented.trim_end()
    }));
    let first_kept = out.iter().position(|line| !line.is_empty());
    let last_kept = out.iter().rposition(|line| !line.is_empty());
    match (first_kept, last_kept) {
        (Some(start), Some(end)) => out[start..=end].join("\n"),
        _ => String::new(),
    }
}

/// One closed plan and its follow-ups.
#[derive(Clone, Debug)]
pub struct Plan {
    /// The plan.
    pub note: NoteObject,
    /// Its last-change date, `YYYY-MM-DD`, as `brief` dates a note.
    pub changed: String,
    /// Its items, in section order.
    pub items: Vec<Item>,
}

/// What `follow-ups` found.
#[derive(Clone, Debug)]
pub struct Answer {
    /// The owning project.
    pub project: NoteObject,
    /// The closed plans carrying at least one item, newest first.
    pub plans: Vec<Plan>,
}

/// The project note an id names.
///
/// # Errors
/// Refuses an id that names no note, and one that names a note of another kind
/// rather than answering with an empty list.
pub fn project_note(root: &Path, id: &str) -> Result<NoteObject> {
    match query::resolve(root, id) {
        Some(note) if note.kind == Kind::Project => Ok(note),
        Some(note) => Err(Error::WrongKind {
            id: id.to_owned(),
            found: note.kind.name(),
            folder: note.kind.folder(),
            wanted: Kind::Project.name(),
        }),
        None => Err(Error::NoSuchNote(id.to_owned())),
    }
}

/// Build the answer for one project.
///
/// # Errors
/// Reports a governed folder that cannot be read.
pub fn run(root: &Path, project: &NoteObject) -> Result<Answer> {
    let index = Index::build(root)?;
    let changed = crate::brief::last_change_dates(root)?;
    let mut plans = vec![];
    for spoke in index.spokes_of(&project.id) {
        if spoke.kind != Kind::Plan || status_of(&spoke.doc) != Some("done") {
            continue;
        }
        let (title, fields, body) = crate::brief::read_spoke(&spoke.path, spoke.kind);
        let items = items(&body);
        if items.is_empty() {
            continue;
        }
        plans.push(Plan {
            note: NoteObject {
                id: spoke.id.clone(),
                kind: spoke.kind,
                path: spoke.path.clone(),
                title,
                fields,
            },
            changed: changed.get(&spoke.path).cloned().unwrap_or_default(),
            items,
        });
    }
    plans.sort_by(|a, b| {
        b.changed
            .cmp(&a.changed)
            .then_with(|| a.note.id.cmp(&b.note.id))
    });
    Ok(Answer {
        project: project.clone(),
        plans,
    })
}

/// A note's `status` scalar.
fn status_of(doc: &Document) -> Option<&str> {
    doc.get("status")?.value.as_scalar()
}
