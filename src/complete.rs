//! Shell completion: what a Tab press offers.
//!
//! The shell asks the binary itself on every Tab press (`COMPLETE=fish
//! mnemex`), so ids come from the vault and kinds, statuses and field names
//! from the schema table as they are now; there is no generated script to go
//! stale. A completer never fails: a vault that is missing or unreadable offers
//! nothing.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::Path;

use clap_complete::engine::CompletionCandidate;

use crate::kind::Kind;
use crate::query;
use crate::spec::Shape;
use crate::vault;

/// The notes of `kind`, or of every kind, whose id or title contains `typed`.
///
/// A match anywhere is offered, not only a prefix, so `lab` completes to
/// `202609231653-home-lab`: the shell replaces the typed word with the id.
#[must_use]
pub fn notes(root: &Path, kind: Option<Kind>, typed: &str) -> Vec<CompletionCandidate> {
    let typed = typed.to_lowercase();
    query::list(root, kind, None)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n.id.contains(&typed) || n.title.to_lowercase().contains(&typed))
        .map(|n| {
            let help = match kind {
                Some(_) => n.title,
                None => format!("{}: {}", n.kind.name(), n.title),
            };
            CompletionCandidate::new(n.id).help(Some(help.into()))
        })
        .collect()
}

/// Every tag some note carries that contains `typed`, once each, sorted.
#[must_use]
pub fn tags(root: &Path, typed: &str) -> Vec<CompletionCandidate> {
    let mut seen = BTreeSet::new();
    for note in query::list(root, None, None).unwrap_or_default() {
        for field in note.fields.into_iter().flatten() {
            if field.key == "tags" {
                seen.extend(field.value.as_list().unwrap_or_default().iter().cloned());
            }
        }
    }
    seen.into_iter()
        .filter(|t| t.contains(typed))
        .map(CompletionCandidate::new)
        .collect()
}

/// The kinds, each with the folder it lands in.
#[must_use]
pub fn kinds() -> Vec<CompletionCandidate> {
    Kind::ALL
        .into_iter()
        .map(|k| CompletionCandidate::new(k.name()).help(Some(format!("{}/", k.folder()).into())))
        .collect()
}

/// Every value of every kind's `status`, each with the kinds that take it.
#[must_use]
pub fn statuses() -> Vec<CompletionCandidate> {
    by_kind(|kind| match kind.spec().field("status").map(|f| f.shape) {
        Some(Shape::Enum(values)) => values.to_vec(),
        _ => vec![],
    })
}

/// Every field of every kind, each with the kinds that have it.
#[must_use]
pub fn fields() -> Vec<CompletionCandidate> {
    by_kind(|kind| kind.spec().fields.iter().map(|f| f.name).collect())
}

/// The names `per_kind` yields across the kinds, once each in first-seen order,
/// each with the kinds that yielded it.
fn by_kind(per_kind: impl Fn(Kind) -> Vec<&'static str>) -> Vec<CompletionCandidate> {
    let mut seen: Vec<(&str, Vec<&str>)> = vec![];
    for kind in Kind::ALL {
        for name in per_kind(kind) {
            match seen.iter_mut().find(|(n, _)| *n == name) {
                Some((_, kinds)) => kinds.push(kind.name()),
                None => seen.push((name, vec![kind.name()])),
            }
        }
    }
    seen.into_iter()
        .map(|(name, kinds)| CompletionCandidate::new(name).help(Some(kinds.join(", ").into())))
        .collect()
}

/// `set`'s field: the fields of the note already named, or of every kind when
/// it names none.
///
/// `before` is the words `set` has before the cursor, its note first.
#[must_use]
pub fn set_field(root: &Path, before: &[String], typed: &str) -> Vec<CompletionCandidate> {
    let candidates = match before.first().and_then(|id| query::resolve(root, id)) {
        Some(note) => note
            .kind
            .spec()
            .fields
            .iter()
            .map(|f| CompletionCandidate::new(f.name))
            .collect(),
        None => fields(),
    };
    with_prefix(candidates, typed)
}

/// `set`'s values, by the field already named: an enum's values, a link's
/// notes of the kind it points at, or the tags in use.
///
/// `before` is the words `set` has before the cursor, its note then its field.
#[must_use]
pub fn set_value(root: &Path, before: &[String], typed: &str) -> Vec<CompletionCandidate> {
    let [id, field, ..] = before else {
        return vec![];
    };
    if field == "tags" {
        return tags(root, typed);
    }
    let Some(spec) = query::resolve(root, id).and_then(|n| n.kind.spec().field(field)) else {
        return vec![];
    };
    match (spec.shape, spec.target) {
        (Shape::Enum(values), _) => with_prefix(
            values
                .iter()
                .copied()
                .map(CompletionCandidate::new)
                .collect(),
            typed,
        ),
        (_, Some(target)) => notes(root, Some(target), typed),
        _ => vec![],
    }
}

/// The candidates that start with `typed`, for a closed set where a match
/// anywhere would only be noise.
fn with_prefix(candidates: Vec<CompletionCandidate>, typed: &str) -> Vec<CompletionCandidate> {
    candidates
        .into_iter()
        .filter(|c| c.get_value().to_string_lossy().starts_with(typed))
        .collect()
}

/// The positional words of `set` before the cursor, read from the command line
/// the shell passes after the first `--`.
///
/// A completer is handed only the word under the cursor; `set`'s value depends
/// on the field before it. `set` has one flag, `--clear`, which takes no value,
/// so every other word after `set` is positional. The last word is the one
/// under the cursor.
#[doc(hidden)]
#[must_use]
pub fn set_words(args: &[String]) -> Vec<String> {
    let before_cursor = args.split_last().map_or(&[][..], |(_, rest)| rest);
    before_cursor
        .iter()
        .skip_while(|a| *a != "--")
        .skip(1)
        .skip_while(|a| *a != "set")
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect()
}

// The `ArgValueCompleter` entry points `cli` attaches. Each reads the one vault
// root and the word under the cursor.

fn in_vault(complete: impl FnOnce(&Path) -> Vec<CompletionCandidate>) -> Vec<CompletionCandidate> {
    vault::root()
        .map(|root| complete(&root))
        .unwrap_or_default()
}

fn line() -> Vec<String> {
    set_words(&std::env::args().collect::<Vec<_>>())
}

/// Any note's id.
#[must_use]
pub fn any_note(typed: &OsStr) -> Vec<CompletionCandidate> {
    in_vault(|root| notes(root, None, &typed.to_string_lossy()))
}

/// A project's id.
#[must_use]
pub fn project(typed: &OsStr) -> Vec<CompletionCandidate> {
    in_vault(|root| notes(root, Some(Kind::Project), &typed.to_string_lossy()))
}

/// A tag in use.
#[must_use]
pub fn tag(typed: &OsStr) -> Vec<CompletionCandidate> {
    in_vault(|root| tags(root, &typed.to_string_lossy()))
}

/// `set`'s field.
#[must_use]
pub fn field(typed: &OsStr) -> Vec<CompletionCandidate> {
    in_vault(|root| set_field(root, &line(), &typed.to_string_lossy()))
}

/// `set`'s values.
#[must_use]
pub fn value(typed: &OsStr) -> Vec<CompletionCandidate> {
    in_vault(|root| set_value(root, &line(), &typed.to_string_lossy()))
}
