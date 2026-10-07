//! The seventeen rules.
//!
//! Rule interaction is a requirement, not an accident: `MX001` and `MX401` each
//! report alone; a file outside the five governed folders produces nothing in a
//! vault check, and `check --path` refuses it, because "not ours to judge" is
//! decided by location and never by content; an empty required scalar is `MX100`
//! and never also `MX102`; container checks run before any shape check below
//! them; and `MX108` precedes `MX404` and `MX405`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::diagnostic::{Code, Diagnostic, Severity};
use crate::error::{Error, Result};
use crate::frontmatter::{self, Field, Position, Value};
use crate::id;
use crate::index::{Index, Note};
use crate::kind::Kind;
use crate::links;
use crate::path;
use crate::spec::Shape;
use crate::vault::{self, GovernedFile};

/// What the filesystem-reading rules need.
#[derive(Clone, Copy, Debug, Default)]
pub struct Env<'a> {
    /// The home directory, for `~/` and `$HOME/` in a project's `path`.
    pub home: Option<&'a Path>,
}

struct Collector<'a> {
    path: &'a Path,
    out: Vec<Diagnostic>,
}

impl<'a> Collector<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path, out: vec![] }
    }

    fn push(&mut self, code: Code, severity: Severity, span: Option<Position>, message: String) {
        self.out.push(Diagnostic {
            path: self.path.to_path_buf(),
            code,
            severity,
            message,
            span,
        });
    }

    fn error(&mut self, code: Code, span: Option<Position>, message: String) {
        self.push(code, Severity::Error, span, message);
    }

    fn warning(&mut self, code: Code, span: Option<Position>, message: String) {
        self.push(code, Severity::Warning, span, message);
    }

    /// Sorted by span, then code; spanless first.
    fn finish(mut self) -> Vec<Diagnostic> {
        self.out.sort_by_key(Diagnostic::sort_key);
        self.out
    }
}

/// Check one governed file.
#[must_use]
pub fn note(index: &Index, env: Env<'_>, file: &GovernedFile) -> Vec<Diagnostic> {
    let mut c = Collector::new(&file.path);

    // MX401 short-circuits: identity is the filename, so a file the filename
    // cannot name has no identity to check anything against.
    if !id::is_valid(&file.stem) {
        c.error(
            Code::Mx401,
            None,
            format!(
                "filename `{}` is not a valid note id (twelve digits, a hyphen, then a lowercase slug); `mnemex adopt {} --kind {}` moves it to a valid one",
                file.stem,
                file.path.display(),
                file.kind.name(),
            ),
        );
        return c.finish();
    }

    let src = match std::fs::read_to_string(&file.path) {
        Ok(s) => s,
        Err(e) => {
            c.error(Code::Mx001, Some(Position::new(1, 1)), e.to_string());
            return c.finish();
        }
    };

    // MX001 short-circuits: an unreadable block reports alone, because no other
    // rule can read the block it would need.
    let doc = match frontmatter::parse(&src) {
        Ok(d) => d,
        Err(e) => {
            c.error(Code::Mx001, Some(e.at), e.message);
            return c.finish();
        }
    };

    let kind = file.kind;
    let note = Note {
        id: file.stem.clone(),
        path: file.path.clone(),
        kind,
        doc,
    };

    for (key, at) in &note.doc.duplicates {
        c.error(
            Code::Mx002,
            Some(*at),
            format!("`{key}` appears more than once; only the last would be kept"),
        );
    }

    for field in &note.doc.fields {
        if field.value == Value::Unsupported {
            c.error(
                Code::Mx003,
                Some(field.anchor()),
                format!("`{}` must be a scalar or a list of scalars", field.key),
            );
        }
        if kind.spec().field(&field.key).is_none() {
            c.warning(
                Code::Mx105,
                Some(field.anchor()),
                format!(
                    "`{}` is not a field of {} {}; it is preserved but unchecked",
                    field.key,
                    kind.article(),
                    kind.name()
                ),
            );
        }
    }

    schema_rules(&mut c, index, &note);
    layout_rules(&mut c, index, env, &note);

    c.finish()
}

fn schema_rules(c: &mut Collector<'_>, index: &Index, note: &Note) {
    for spec in note.kind.spec().fields {
        let Some(field) = note.doc.get(spec.name) else {
            if spec.required {
                c.error(
                    Code::Mx100,
                    None,
                    format!(
                        "`{}` is required for {} {}",
                        spec.name,
                        note.kind.article(),
                        note.kind.name()
                    ),
                );
            }
            continue;
        };

        // An empty scalar is the only shape that counts as absent, and it is
        // MX100 alone — missing and invalid are different findings.
        if field.value.is_empty_scalar() {
            if spec.required {
                c.error(
                    Code::Mx100,
                    Some(field.anchor()),
                    format!(
                        "`{}` is required for {} {}",
                        spec.name,
                        note.kind.article(),
                        note.kind.name()
                    ),
                );
            }
            continue;
        }

        if field.value == Value::Unsupported {
            continue; // Already MX003; there is nothing below it to check.
        }

        // Container checks run first: if a value is in the wrong container, no
        // shape check below it is meaningful.
        match (&field.value, spec.shape.is_list()) {
            (Value::List(_), false) => {
                c.error(
                    Code::Mx104,
                    Some(field.anchor()),
                    format!(
                        "`{}` must be a single {}, not a list",
                        spec.name,
                        spec.shape.tag()
                    ),
                );
                continue;
            }
            (Value::Scalar(_), true) => {
                c.error(
                    Code::Mx104,
                    Some(field.anchor()),
                    format!("`{}` must be a list, not a single value", spec.name),
                );
                continue;
            }
            _ => {}
        }

        match spec.shape {
            Shape::Enum(values) => enum_rule(c, field, spec.name, values),
            Shape::WikiLink => project_rule(c, index, &note.id, field),
            Shape::Remote => remote_rule(c, field, spec.name, &note.id),
            Shape::UrlList => refs_rule(c, field, &note.id),
            Shape::Text | Shape::TextList => {}
        }
    }
}

/// `MX200`, `MX202`, `MX204`: a `project` field that is not a well-formed link
/// to a project note.
fn project_rule(c: &mut Collector<'_>, index: &Index, owner: &str, field: &Field) {
    let Some(text) = field.value.as_scalar() else {
        return;
    };
    let at = field.anchor();
    match links::target(text) {
        None => c.error(
            Code::Mx200,
            Some(at),
            format!("`project` must be a wiki-link like `[[id]]`, found `{text}`"),
        ),
        Some(target) if !index.contains(target) => c.error(
            Code::Mx202,
            Some(at),
            format!(
                "`project` points at `{target}`, which is not a note in this vault; `mnemex set {owner} project <project-id>` names one"
            ),
        ),
        Some(target) => {
            if let Some(found) = index.kind_of(target)
                && found != Kind::Project
            {
                c.error(
                    Code::Mx204,
                    Some(at),
                    format!(
                        "`project` must name a project, but `{target}` is {} {}",
                        found.article(),
                        found.name()
                    ),
                );
            }
        }
    }
}

fn enum_rule(c: &mut Collector<'_>, field: &Field, name: &str, values: &[&str]) {
    let Some(found) = field.value.as_scalar() else {
        return;
    };
    if values.contains(&found) {
        return;
    }
    let listed = values
        .iter()
        .map(|v| format!("`{v}`"))
        .collect::<Vec<_>>()
        .join(", ");
    c.error(
        Code::Mx102,
        Some(field.anchor()),
        format!("`{name}` must be one of {listed}, found `{found}`"),
    );
}

/// A remote is stored canonically, so that two spellings of one repository
/// compare equal. A spelling the verbs would have canonicalised is
/// still a finding: the block was written past them.
fn remote_rule(c: &mut Collector<'_>, field: &Field, name: &str, id: &str) {
    let Some(found) = field.value.as_scalar() else {
        return;
    };
    if crate::git::is_canonical(found) {
        return;
    }
    c.error(
        Code::Mx108,
        Some(field.anchor()),
        format!(
            "`{name}` must be a canonical remote as `host/owner/name`, found `{found}`; `mnemex set {id} {name} <url>` canonicalises one"
        ),
    );
}

/// `MX110`: a `refs` entry that would not survive normalisation. The verbs
/// normalise on write, so a bad entry is one written past them.
fn refs_rule(c: &mut Collector<'_>, field: &Field, id: &str) {
    let Some(items) = field.value.as_list() else {
        return;
    };
    for (i, text) in items.iter().enumerate() {
        if crate::refs::is_canonical(text) {
            continue;
        }
        c.error(
            Code::Mx110,
            Some(field.item_anchor(i)),
            format!(
                "`refs` entry must be a normalised `http(s)://` URL with a path and no credentials, found `{text}`; `mnemex set {id} refs <url>...` normalises the list"
            ),
        );
    }
}

/// The three layout rules, all warnings, each for its own reason.
///
/// They are skipped when the vault root is not on this filesystem: checking a
/// note out of its vault — a fixture, a synthetic block — is not evidence of
/// drift.
fn layout_rules(c: &mut Collector<'_>, index: &Index, env: Env<'_>, note: &Note) {
    if note.kind != Kind::Project || !index.root.is_dir() {
        return;
    }
    let Some(field) = note.doc.get("path") else {
        return;
    };
    let Some(text) = field.value.as_scalar() else {
        return;
    };
    let Some(dir) = path::resolve(&index.root, env.home, text) else {
        return;
    };
    let at = Some(field.anchor());

    if !dir.is_dir() {
        c.warning(
            Code::Mx403,
            at,
            format!(
                "`path` names `{}`, which is not a directory here",
                dir.display()
            ),
        );
        return;
    }
    repo_rule(c, note, &dir, at);
}

/// `MX405` when `path` names a checkout and the note has no `repo`; `MX404`
/// when the identity the note claims is no remote of the checkout its `path`
/// names.
///
/// The claim is matched against **every** remote, not only `origin`: a fork
/// kept beside an upstream, and a checkout whose remotes are named for the
/// layers it serves, are both still the one project the note governs.
///
/// Both sides must be canonical for the comparison to mean anything: an
/// uncanonical `repo` is `MX108`'s finding, and a checkout whose every remote
/// is uncanonical has no identity to compare.
fn repo_rule(c: &mut Collector<'_>, note: &Note, dir: &Path, path_at: Option<Position>) {
    if !crate::git::is_checkout(dir) {
        return;
    }
    let found = crate::git::remotes(dir);
    let Some(field) = note.doc.get("repo") else {
        warn_missing_repo(c, note, dir, path_at, &found);
        return;
    };
    if field.value.is_empty_scalar() {
        warn_missing_repo(c, note, dir, Some(field.anchor()), &found);
        return;
    }
    let Some(claimed) = field
        .value
        .as_scalar()
        .filter(|r| crate::git::is_canonical(r))
    else {
        return;
    };
    if found.iter().any(|r| r == claimed) {
        return;
    }
    let message = match found.as_slice() {
        [] => return,
        [only] => format!(
            "`repo` is `{claimed}`, but `{}` has remote `{only}`; one of the two is out of date",
            dir.display()
        ),
        _ => format!(
            "`repo` is `{claimed}`, but `{}` has remotes {}, none of them it, so one side is out of date",
            dir.display(),
            listed(&found)
        ),
    };
    c.warning(Code::Mx404, Some(field.anchor()), message);
}

/// The remotes as a reader names them: `` `a`, `b` ``.
fn listed(remotes: &[String]) -> String {
    remotes
        .iter()
        .map(|r| format!("`{r}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn warn_missing_repo(
    c: &mut Collector<'_>,
    note: &Note,
    dir: &Path,
    at: Option<Position>,
    found: &[String],
) {
    // A repair may name one remote, and the derivation names `origin`: which of
    // several remotes a project *is* is the writer's call, not this tool's.
    let suggested = crate::git::origin(dir).or_else(|| match found {
        [only] => Some(only.clone()),
        _ => None,
    });
    let repair = match suggested {
        Some(found) => format!("`mnemex set {} repo {found}` records it", note.id),
        // A checkout with no remote (or none this tool can store) has nothing to
        // suggest, so the repair must not name one.
        None if found.is_empty() => format!(
            "the checkout has no remote this tool can store, so set `repo` by hand with `mnemex set {} repo <remote>`",
            note.id
        ),
        None => format!(
            "the checkout's remotes are {}, and none is `origin`, so name the one the project is with `mnemex set {} repo <remote>`",
            listed(found),
            note.id
        ),
    };
    c.warning(
        Code::Mx405,
        at,
        format!(
            "`path` names `{}`, a git checkout, but `repo` is not set; {repair}",
            dir.display()
        ),
    );
}

/// Check a whole vault. Diagnostics are grouped by note, and the notes appear in
/// index order.
///
/// # Errors
/// Reports a governed folder that cannot be read.
pub fn root(root: &Path, env: Env<'_>) -> Result<Vec<Diagnostic>> {
    // One walk serves both the index and the duplicate-id scan.
    let files = vault::governed_files(root)?;
    let index = Index::from_files(root, &files);
    let duplicates = duplicate_ids(&files);
    let mut out = vec![];
    for file in &files {
        let mut ds = note(&index, env, file);
        if let Some(message) = duplicates.get(file.stem.as_str()) {
            // A note's diagnostics sort together, MX406 among them.
            ds.push(duplicate_id_diagnostic(&file.path, message));
            ds.sort_by_key(Diagnostic::sort_key);
        }
        out.extend(ds);
    }
    Ok(out)
}

/// Check one note of the vault at `root`, cross-file rules included — so the
/// write hook catches a dangling link as soon as it is written.
///
/// A missing note of that vault still reports `MX001` rather than turning into
/// an I/O refusal.
///
/// # Errors
/// Refuses a path that is not a note of that vault (see
/// [`vault::governed_file`]), and reports a vault that cannot be read.
pub fn path(note_path: &Path, root: &Path, env: Env<'_>) -> Result<Vec<Diagnostic>> {
    let Some(file) = vault::governed_file(root, note_path) else {
        return Err(Error::NotANote {
            path: note_path.to_path_buf(),
            root: root.to_path_buf(),
        });
    };
    // One walk serves both the index and the duplicate-id scan.
    let files = vault::governed_files(root)?;
    let index = Index::from_files(root, &files);
    let duplicates = duplicate_ids(&files);
    let mut out = note(&index, env, &file);
    if let Some(message) = duplicates.get(file.stem.as_str()) {
        out.push(duplicate_id_diagnostic(&file.path, message));
        out.sort_by_key(Diagnostic::sort_key);
    }
    Ok(out)
}

/// `MX406`: the ids a vault holds more than once, mapped to the diagnostic
/// message that names the repair.
///
/// The id is the filename stem, so the collision is read from the governed walk
/// directly rather than from the index: a duplicate whose frontmatter will not
/// parse still collides, and still has to be found.
fn duplicate_ids(files: &[GovernedFile]) -> BTreeMap<String, String> {
    let mut by_stem: BTreeMap<&str, Vec<&GovernedFile>> = BTreeMap::new();
    for file in files {
        if id::is_valid(&file.stem) {
            by_stem.entry(&file.stem).or_default().push(file);
        }
    }
    let mut out = BTreeMap::new();
    for (stem, group) in by_stem {
        if group.len() < 2 {
            continue;
        }
        let folders = group
            .iter()
            .map(|f| f.kind.folder())
            .collect::<Vec<_>>()
            .join("`, `");
        out.insert(
            stem.to_owned(),
            format!(
                "`{stem}` names more than one note (in `{folders}`); ids must be unique across the vault, so `mnemex rename {stem} <new title>` one of them"
            ),
        );
    }
    out
}

fn duplicate_id_diagnostic(path: &Path, message: &str) -> Diagnostic {
    Diagnostic {
        path: path.to_path_buf(),
        code: Code::Mx406,
        severity: Severity::Error,
        message: message.to_owned(),
        span: None,
    }
}
