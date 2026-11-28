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
    /// Its text: the bullet and its continuation lines, joined into one line.
    pub text: String,
}

/// The follow-up items of a plan body.
///
/// The section runs from a `## Follow-ups` line to the next `## ` line or the
/// end of the body. A line inside a code fence is never a heading or a bullet,
/// but an item's own block is text: an indented fence and its lines continue
/// the item. A fence marker at column zero is a section-level block, so it ends
/// the item and its lines belong to none. An item is a top-level `- ` bullet
/// plus its following indented lines; a blank line between them does not end
/// it. Anything else is prose and is ignored, and a bullet with no text at all
/// is not work, so it is no item.
#[must_use]
pub fn items(body: &str) -> Vec<Item> {
    let mut fences = Fences::default();
    let mut found = vec![];
    let mut current: Option<String> = None;
    let mut in_section = false;
    for line in body.lines() {
        let prose = fences.is_prose(line);
        if prose {
            if line.trim_end() == HEADING {
                if in_section {
                    break;
                }
                in_section = true;
                continue;
            }
            if !in_section {
                continue;
            }
            if line.starts_with("## ") {
                break;
            }
            if let Some(text) = line.strip_prefix("- ") {
                close(&mut found, &mut current);
                current = Some(text.trim().to_owned());
                continue;
            }
        } else if !in_section {
            continue;
        }
        // Indentation alone decides a continuation, a fenced block's lines
        // included: only structure has to respect the fence.
        if line.starts_with([' ', '\t']) {
            let continuation = line.trim();
            if let Some(text) = &mut current
                && !continuation.is_empty()
            {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(continuation);
            }
            continue;
        }
        if prose && line.trim().is_empty() {
            // A blank line ends nothing by itself: the next line decides
            // whether the item goes on.
            continue;
        }
        // Any other line that is neither a bullet nor indented ends the item,
        // a fence marker at column zero included: a section-level block is not
        // part of a bullet's text.
        close(&mut found, &mut current);
    }
    close(&mut found, &mut current);
    found
}

/// Finish the item under construction, numbering it by its position.
fn close(found: &mut Vec<Item>, current: &mut Option<String>) {
    if let Some(text) = current.take()
        && !text.is_empty()
    {
        found.push(Item {
            n: found.len() + 1,
            text,
        });
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
