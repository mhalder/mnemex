//! `resolve`, `list`, `show` and `project`.
//!
//! The queries exist so that nothing outside the binary carries its own copy of
//! the vault layout. A hook or a status line that hardcodes the governed folder
//! list is a copy, and a copy is a thing that drifts.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::frontmatter::{self, Document, Field};
use crate::id;
use crate::index::Index;
use crate::kind::Kind;
use crate::links;
use crate::markdown;
use crate::path;

/// What a note with no `# ` heading renders as.
pub const UNTITLED: &str = "(untitled)";

/// A note object, as every query envelope carries it.
///
/// `fields` holds every frontmatter field present, in canonical order; it is
/// `None` when the frontmatter does not parse. Consumers never open a note to
/// learn a field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteObject {
    /// Its id.
    pub id: String,
    /// Its kind.
    pub kind: Kind,
    /// Its path.
    pub path: PathBuf,
    /// Its first `# ` heading, or [`UNTITLED`].
    pub title: String,
    /// Every frontmatter field, in canonical order.
    pub fields: Option<Vec<Field>>,
}

/// Read one note file once: its title and its frontmatter fields.
fn read_note(path: &Path, kind: Kind) -> (String, Option<Vec<Field>>) {
    let Ok(src) = std::fs::read_to_string(path) else {
        return (UNTITLED.to_owned(), None);
    };
    let body = frontmatter::body_after_block(&src);
    let title = markdown::first_heading(body.lines()).unwrap_or_else(|| UNTITLED.to_owned());
    let fields = frontmatter::parse(&src)
        .ok()
        .map(|doc| canonical_fields(kind, &doc));
    (title, fields)
}

/// A note object for a file already known to exist.
#[must_use]
pub fn note_object(kind: Kind, path: PathBuf, id: String) -> NoteObject {
    let (title, fields) = read_note(&path, kind);
    NoteObject {
        id,
        kind,
        path,
        title,
        fields,
    }
}

/// A document's fields in canonical order: known fields in schema order, then
/// unknown fields in read order.
#[must_use]
pub fn canonical_fields(kind: Kind, doc: &Document) -> Vec<Field> {
    let spec = kind.spec();
    let mut out = Vec::new();
    for f in spec.fields {
        if let Some(field) = doc.get(f.name) {
            out.push(field.clone());
        }
    }
    for field in &doc.fields {
        if spec.field(&field.key).is_none() {
            out.push(field.clone());
        }
    }
    out
}

/// Find a note by id: probe the five folders in layout order, first hit wins.
///
/// Only a valid id resolves, so `resolve` finds exactly the notes `list` and
/// `show` can reach.
#[must_use]
pub fn resolve(root: &Path, id: &str) -> Option<NoteObject> {
    resolve_in(root, id, &Kind::ALL)
}

/// Find a note by id, probing only the given kinds.
#[must_use]
pub fn resolve_in(root: &Path, id: &str, kinds: &[Kind]) -> Option<NoteObject> {
    if !id::is_valid(id) {
        return None;
    }
    for kind in kinds {
        let path = root.join(kind.folder()).join(format!("{id}.md"));
        if path.is_file() {
            return Some(note_object(*kind, path, id.to_owned()));
        }
    }
    None
}

/// Every note a valid id names, in index order, optionally one kind or one
/// project.
///
/// A note whose frontmatter does not parse is listed too: it is still a note,
/// and hiding it would make its `MX001` the only sign it exists. `--project`
/// keeps the notes whose `project` field names the id, which a project itself
/// never does.
///
/// # Errors
/// Reports a governed folder that cannot be read.
pub fn list(root: &Path, kind: Option<Kind>, project: Option<&str>) -> Result<Vec<NoteObject>> {
    if let Some(project_id) = project {
        let index = Index::build(root)?;
        return Ok(index
            .spokes_of(project_id)
            .into_iter()
            .filter(|n| kind.is_none_or(|k| k == n.kind))
            .map(|n| note_object(n.kind, n.path.clone(), n.id.clone()))
            .collect());
    }
    Ok(crate::vault::governed_files(root)?
        .into_iter()
        .filter(|f| id::is_valid(&f.stem) && kind.is_none_or(|k| k == f.kind))
        .map(|f| note_object(f.kind, f.path.clone(), f.stem.clone()))
        .collect())
}

/// The full ids of every note whose slug half is `value`, restricted to `kind`
/// when given, in index order.
///
/// A slug is not an id and is not unique: two notes with the same title minted
/// at different stamps share one. Every match is returned, so a caller can say
/// there is more than one rather than pick one silently.
#[must_use]
pub fn slug_matches(root: &Path, kind: Option<Kind>, value: &str) -> Vec<String> {
    if id::is_valid(value) {
        return vec![];
    }
    list(root, kind, None)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| id::slug(&n.id) == Some(value))
        .map(|n| n.id)
        .collect()
}

/// The hint for a value a verb was handed as an id: `value` is a slug, and these
/// are the full ids it matches. `None` when `value` is already an id, or names
/// nothing.
#[must_use]
pub fn slug_hint(root: &Path, kind: Option<Kind>, value: &str) -> Option<String> {
    let matches = slug_matches(root, kind, value);
    match matches.as_slice() {
        [] => None,
        [one] => Some(format!("`{value}` is a slug, not an id; use `{one}`")),
        many => Some(format!(
            "`{value}` is a slug, not an id; it matches {}",
            many.iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The ways `project` can answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectAnswer {
    /// The directory asked about, canonicalised.
    pub dir: PathBuf,
    /// `"repo"` or `"path"` when one matched, `None` otherwise.
    pub via: Option<&'static str>,
    /// The remote or stored path that answered, for the `via` line.
    pub detail: Option<String>,
    /// The owning project, when one won.
    pub note: Option<NoteObject>,
    /// The projects still in the running when more than one could answer.
    pub candidates: Vec<NoteObject>,
    /// Why there is no answer, when there is none.
    pub issue: Option<String>,
}

/// The first ancestor (inclusive) of `dir` that is a git checkout.
fn first_checkout(dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if crate::git::is_checkout(d) {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

/// A scalar field's text, from a note object.
fn field_scalar<'a>(note: &'a NoteObject, name: &str) -> Option<&'a str> {
    note.fields
        .as_ref()?
        .iter()
        .find(|f| f.key == name)
        .and_then(|f| f.value.as_scalar())
}

/// Find the project that owns a directory.
///
/// 1. **Repo match.** Walk up to the first ancestor holding `.git`, compute its
///    canonical `origin`, and collect the projects whose `repo` equals it.
/// 2. **Path match.** For every project with a `path`, resolve it and collect
///    those where the directory equals it or starts with it; the longest
///    resolved path wins.
/// 3. **Choice.** A path match wins over a repo match, because a monorepo may
///    hold several projects. More than one candidate at the winning level is
///    ambiguity.
///
/// The scan touches `projects/` only, so cost is flat in vault size.
///
/// # Errors
/// Reports a `projects/` folder that cannot be read.
pub fn project(root: &Path, dir: &Path, home: Option<&Path>) -> Result<ProjectAnswer> {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let dir = canonical(dir);

    let projects: Vec<NoteObject> = crate::vault::governed_files(root)?
        .into_iter()
        .filter(|f| f.kind == Kind::Project && id::is_valid(&f.stem))
        .map(|f| note_object(f.kind, f.path.clone(), f.stem.clone()))
        .collect();

    // Path matches first: more specific than a repo.
    let mut path_matches: Vec<(&NoteObject, PathBuf)> = vec![];
    for p in &projects {
        let Some(text) = field_scalar(p, "path") else {
            continue;
        };
        let Some(resolved) = path::resolve(root, home, text) else {
            continue;
        };
        let resolved = canonical(&resolved);
        if dir == resolved || dir.starts_with(&resolved) {
            path_matches.push((p, resolved));
        }
    }
    if let Some(longest) = path_matches
        .iter()
        .map(|(_, r)| r.components().count())
        .max()
    {
        let winners: Vec<NoteObject> = path_matches
            .iter()
            .filter(|(_, r)| r.components().count() == longest)
            .map(|(p, _)| (*p).clone())
            .collect();
        let detail = field_scalar(&winners[0], "path")
            .unwrap_or_default()
            .to_owned();
        return Ok(choose(dir, winners, "path", detail));
    }

    // Repo match.
    let mut repo_matches: Vec<NoteObject> = vec![];
    let mut repo = String::new();
    if let Some(checkout) = first_checkout(&dir)
        && let Some(origin) = crate::git::origin(&checkout)
    {
        repo = origin;
        for p in &projects {
            if field_scalar(p, "repo") == Some(repo.as_str()) {
                repo_matches.push(p.clone());
            }
        }
    }
    Ok(choose(dir, repo_matches, "repo", repo))
}

fn choose(
    dir: PathBuf,
    mut winners: Vec<NoteObject>,
    via: &'static str,
    detail: String,
) -> ProjectAnswer {
    winners.sort_by(|a, b| a.id.cmp(&b.id));
    match winners.len() {
        1 => ProjectAnswer {
            dir,
            via: Some(via),
            detail: Some(detail),
            note: winners.pop(),
            candidates: vec![],
            issue: None,
        },
        0 => {
            let issue = format!(
                "no project owns `{}`: no project's `repo` matches its checkout and no project's `path` contains it",
                dir.display()
            );
            ProjectAnswer {
                dir,
                via: None,
                detail: None,
                note: None,
                candidates: vec![],
                issue: Some(issue),
            }
        }
        _ => ProjectAnswer {
            dir,
            via: None,
            detail: None,
            note: None,
            candidates: winners,
            issue: None,
        },
    }
}

/// A project's spokes, grouped by kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spokes {
    /// `plans/`, in index order.
    pub plans: Vec<NoteObject>,
    /// `adrs/`, in index order.
    pub adrs: Vec<NoteObject>,
    /// `memories/`, in index order.
    pub memories: Vec<NoteObject>,
    /// `contexts/`, in index order.
    pub contexts: Vec<NoteObject>,
}

/// What `show` found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowAnswer {
    /// The note.
    pub note: NoteObject,
    /// The id the note's `project` names — its own, for a project.
    pub project_id: String,
    /// The owning project, resolved; `None` for a dangling id or a malformed
    /// `project` field.
    pub project: Option<NoteObject>,
    /// The owning project's spokes, grouped by kind.
    pub spokes: Spokes,
}

/// What a note belongs to, and its project's spokes.
///
/// This is the verb that makes a note readable on its own: a plan carries no
/// links of its own, so without it, reading `plans/x.md` reveals nothing about
/// its project, its ADRs or its siblings.
///
/// # Errors
/// Reports a vault that cannot be read, and a note whose frontmatter does not
/// parse: it exists, so answering "no such note" would be wrong, but it has no
/// links to show.
pub fn show(root: &Path, id: &str) -> Result<Option<ShowAnswer>> {
    let index = Index::build(root)?;
    let Some(note) = index.get(id) else {
        if index.contains(id)
            && let Some(found) = resolve(root, id)
        {
            return Err(Error::NoteDoesNotParse {
                id: id.to_owned(),
                path: found.path,
            });
        }
        return Ok(None);
    };
    let note = note_object(note.kind, note.path.clone(), note.id.clone());

    let (project_id, project) = if note.kind == Kind::Project {
        (id.to_owned(), Some(note.clone()))
    } else {
        match note
            .fields
            .as_ref()
            .and_then(|fs| fs.iter().find(|f| f.key == "project"))
            .and_then(|f| f.value.as_scalar())
            .and_then(links::target)
        {
            Some(pid) => (pid.to_owned(), resolve_in(root, pid, &[Kind::Project])),
            None => (String::new(), None),
        }
    };

    let mut spokes = Spokes::default();
    if let Some(project) = &project {
        for spoke in index.spokes_of(&project.id) {
            let obj = note_object(spoke.kind, spoke.path.clone(), spoke.id.clone());
            match spoke.kind {
                Kind::Plan => spokes.plans.push(obj),
                Kind::Adr => spokes.adrs.push(obj),
                Kind::Memory => spokes.memories.push(obj),
                Kind::Context => spokes.contexts.push(obj),
                Kind::Project => {}
            }
        }
    }

    Ok(Some(ShowAnswer {
        note,
        project_id,
        project,
        spokes,
    }))
}
