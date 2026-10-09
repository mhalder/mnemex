//! The authoring verbs.
//!
//! Every metadata change goes through a typed verb that parses, mutates,
//! validates, then writes. Shared behaviour: a verb reports what it did, then
//! the affected note's path, then — if the note is not clean — the diagnostic
//! report. The verbs succeed even when the note they wrote still trips a rule:
//! the verb worked, and a deliberately blank required field is guidance, not
//! failure.

use core::fmt::Write as _;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::check::{self, Env};
use crate::diagnostic::Diagnostic;
use crate::error::{Error, Result};
use crate::frontmatter::{self, Document, Value};
use crate::id;
use crate::index::{Index, Note};
use crate::kind::Kind;
use crate::links;
use crate::markdown::{self, Fences};
use crate::path;
use crate::scaffold;
use crate::spec::{Default_, Shape};

/// What a verb did, and what the tool has to say about the note afterwards.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// What the verb did.
    pub headline: String,
    /// The affected note.
    pub path: PathBuf,
    /// Anything else worth reporting: the count of links a rename updated, a
    /// body that still mentions an old id.
    pub notes: Vec<String>,
    /// The affected note's diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Where a verb runs.
#[derive(Clone, Copy, Debug)]
pub struct Ctx<'a> {
    /// The vault root.
    pub root: &'a Path,
    /// The home directory, for `~/` and `$HOME/` in a project's `path`.
    pub home: Option<&'a Path>,
    /// The `YYYYMMDDHHMM` local-time stamp a new id starts with.
    pub stamp: &'a str,
}

impl Ctx<'_> {
    fn env(&self) -> Env<'_> {
        Env { home: self.home }
    }

    fn index(&self) -> Result<Index> {
        Index::build(self.root)
    }
}

/// What `new` was asked for.
#[derive(Clone, Copy, Debug, Default)]
pub struct NewArgs<'a> {
    /// `--project`.
    pub project: Option<&'a str>,
    /// `--status`.
    pub status: Option<&'a str>,
    /// `--tag`, repeated.
    pub tags: &'a [String],
    /// `--ref`, repeated.
    pub refs: &'a [String],
    /// `--path`.
    pub path: Option<&'a str>,
    /// `--repo`.
    pub repo: Option<&'a str>,
}

fn note_of<'a>(index: &'a Index, id: &str) -> Result<&'a Note> {
    index
        .get(id)
        .ok_or_else(|| Error::NoSuchNote(id.to_owned()))
}

fn expect_kind<'a>(index: &'a Index, id: &str, wanted: Kind) -> Result<&'a Note> {
    let note = note_of(index, id)?;
    if note.kind == wanted {
        return Ok(note);
    }
    Err(Error::WrongKind {
        id: id.to_owned(),
        found: note.kind.name(),
        folder: note.kind.folder(),
        wanted: wanted.name(),
    })
}

fn writable(note: &Note) -> Result<Document> {
    if note.doc.is_renderable() {
        Ok(note.doc.clone())
    } else {
        Err(Error::Unrenderable(note.path.clone()))
    }
}

fn save(path: &Path, kind: Kind, doc: &Document) -> Result<()> {
    write_atomically(path, &doc.render(kind))
}

/// Write `contents` to `path` without ever leaving it half-written.
///
/// A note that is a symlink is written through to its target, and an existing
/// note keeps its permissions: a rename puts a new file in place, so neither
/// would survive otherwise.
fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    // Renaming over a link would replace the link with a regular file and fork
    // the note from its target, so the target is what gets replaced.
    let path = &canonical(path);
    // Write a sibling temp file and rename it over the target: a reader never
    // sees a half-written note, and a crash never leaves one truncated. The
    // temp name is not a `.md`, so the governed walk never mistakes it for a
    // note while it exists.
    let dir = path
        .parent()
        .ok_or_else(|| Error::io(path, std::io::Error::other("note has no parent directory")))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::io(path, std::io::Error::other("note has no file name")))?;
    let tmp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    // The temp file is created with default permissions, so a note kept
    // private would come back readable unless its own are copied across.
    let staged = std::fs::write(&tmp, contents).and_then(|()| match std::fs::metadata(path) {
        Ok(meta) => std::fs::set_permissions(&tmp, meta.permissions()),
        Err(_) => Ok(()),
    });
    if let Err(e) = staged {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::io(&tmp, e));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::io(path, e));
    }
    Ok(())
}

/// Refuse to mint an id that already names a note anywhere in the vault.
///
/// An id is unique across all five governed folders, not just within one, so
/// a mint probes the whole vault before writing. `except` is the path being
/// renamed away from, which legitimately holds the old id; every other governed
/// file with the stem is a collision.
fn ensure_id_free(root: &Path, new_id: &str, except: Option<&Path>) -> Result<()> {
    let existing = crate::vault::governed_files(root)?
        .into_iter()
        .find(|f| f.stem == new_id && except.is_none_or(|p| p != f.path.as_path()));
    if let Some(existing) = existing {
        return Err(Error::DuplicateId {
            id: new_id.to_owned(),
            folder: existing.kind.folder(),
        });
    }
    Ok(())
}

/// The slug a title mints an id from, refusing a title that is not one line of
/// text or has nothing to slugify. The title is also the body heading, where a
/// newline would break it across lines.
fn slug_of(title: &str) -> Result<String> {
    if title.chars().any(char::is_control) {
        return Err(Error::TitleHasControlCharacter(title.to_owned()));
    }
    let slug = id::slugify(title);
    if slug.is_empty() {
        return Err(Error::TitleHasNoSlug(title.to_owned()));
    }
    Ok(slug)
}

fn outcome(ctx: Ctx<'_>, headline: String, path: PathBuf, notes: Vec<String>) -> Result<Outcome> {
    let diagnostics = check::path(&path, ctx.root, ctx.env())?;
    Ok(Outcome {
        headline,
        path,
        notes,
        diagnostics,
    })
}

/// The validations every mint and `adopt` run before writing anything: the
/// `--project` requirement, `--status`/`--ref` applicability, and
/// `--path`/`--repo` which only a project carries.
fn validate_new_args(kind: Kind, args: NewArgs<'_>) -> Result<()> {
    if kind == Kind::Project {
        if args.project.is_some() {
            return Err(Error::ProjectNotApplicable);
        }
    } else if args.project.is_none() {
        return Err(Error::ProjectRequired { kind: kind.name() });
    }

    match (matches!(kind, Kind::Plan | Kind::Adr), args.status) {
        (false, Some(_)) => return Err(Error::StatusNotApplicable { kind: kind.name() }),
        (true, Some(found)) => {
            if let Some(Shape::Enum(values)) = kind.spec().field("status").map(|f| f.shape)
                && !values.contains(&found)
            {
                return Err(Error::OffEnum {
                    found: found.to_owned(),
                    field: "status",
                    values,
                });
            }
        }
        _ => {}
    }

    let refs_allowed = matches!(kind, Kind::Plan | Kind::Adr | Kind::Memory);
    if !args.refs.is_empty() && !refs_allowed {
        return Err(Error::RefsNotApplicable { kind: kind.name() });
    }
    if args.path.is_some() && kind != Kind::Project {
        return Err(Error::PathNotApplicable { kind: kind.name() });
    }
    if args.repo.is_some() && kind != Kind::Project {
        return Err(Error::RepoNotApplicable { kind: kind.name() });
    }
    if args.tags.iter().any(|t| t.trim().is_empty()) {
        return Err(Error::BlankListValue("tags"));
    }
    if args.refs.iter().any(|r| r.trim().is_empty()) {
        return Err(Error::BlankListValue("refs"));
    }
    Ok(())
}

/// The frontmatter a new note starts with, from the schema table.
fn fields_for(
    kind: Kind,
    project: Option<&str>,
    status: Option<&str>,
    tags: &[String],
    refs: Option<&[String]>,
    path: Option<&str>,
    repo: Option<&str>,
) -> Document {
    let mut doc = Document {
        fields: vec![],
        duplicates: vec![],
        body: String::new(),
    };
    for f in kind.spec().fields {
        let value = match f.name {
            "project" => project.map(|p| Value::Scalar(p.to_owned())),
            "status" => status
                .map(|s| Value::Scalar(s.to_owned()))
                .or_else(|| match f.default {
                    Default_::Scalar(v) => Some(Value::Scalar(v.to_owned())),
                    Default_::None => None,
                }),
            "tags" if tags.is_empty() => None,
            "tags" => Some(Value::List(tags.to_vec())),
            "refs" => refs.map(|r| Value::List(r.to_vec())),
            "path" => path.map(|p| Value::Scalar(p.to_owned())),
            "repo" => repo.map(|r| Value::Scalar(r.to_owned())),
            _ => None,
        };
        if let Some(value) = value {
            doc.set(f.name, value);
        }
    }
    doc
}

/// Write a minted note.
fn write_mint(ctx: Ctx<'_>, mint: Mint) -> Result<Outcome> {
    if let Some(parent) = mint.path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    save(&mint.path, mint.kind, &mint.doc)?;
    outcome(
        ctx,
        format!("created {}", mint.new_id),
        mint.path,
        mint.notes,
    )
}

/// Everything [`write_mint`] needs to write one minted note.
struct Mint {
    kind: Kind,
    doc: Document,
    new_id: String,
    path: PathBuf,
    notes: Vec<String>,
}

/// The project link a non-project note names, wrapped as `[[id]]`.
fn project_link(index: &Index, kind: Kind, project: Option<&str>) -> Result<Option<String>> {
    if kind == Kind::Project {
        return Ok(None);
    }
    let target = expect_kind(
        index,
        project.expect("validated by validate_new_args"),
        Kind::Project,
    )?;
    Ok(Some(links::wrap(&target.id)))
}

/// A normalised, deduplicated `refs` list.
fn refs_list(values: &[String]) -> Result<Value> {
    if values.iter().any(|v| v.trim().is_empty()) {
        return Err(Error::BlankListValue("refs"));
    }
    let mut out: Vec<String> = Vec::with_capacity(values.len());
    for v in values {
        let canonical = crate::refs::canonical(v).ok_or_else(|| Error::NotAUrl(v.clone()))?;
        if !out.contains(&canonical) {
            out.push(canonical);
        }
    }
    Ok(Value::List(out))
}

// --- new ----------------------------------------------------------------

/// Mint a note: validate, then write frontmatter built from the schema table
/// plus the body scaffold.
///
/// # Errors
/// Refuses a bad `--project`, `--status`, `--ref`, a `--path`/`--repo` on any
/// kind but project, a title that slugifies to nothing, and a file that already
/// exists.
pub fn new(ctx: Ctx<'_>, kind: Kind, title: &str, args: NewArgs<'_>) -> Result<Outcome> {
    validate_new_args(kind, args)?;
    // Canonicalised before anything is written, so a remote the tool cannot
    // store never costs a note.
    let repo = args.repo.map(canonical_remote).transpose()?;
    let path = args.path.map(|p| path::stored(p, ctx.home));
    let refs = if args.refs.is_empty() {
        None
    } else {
        Some(refs_list(args.refs)?)
    };

    let index = ctx.index()?;
    let project = project_link(&index, kind, args.project)?;

    let slug = slug_of(title)?;
    let new_id = format!("{}-{slug}", ctx.stamp);
    let note_path = ctx.root.join(kind.folder()).join(format!("{new_id}.md"));
    if note_path.exists() {
        return Err(Error::AlreadyExists(note_path));
    }
    ensure_id_free(ctx.root, &new_id, None)?;

    let mut doc = fields_for(
        kind,
        project.as_deref(),
        args.status,
        args.tags,
        refs.as_ref().and_then(Value::as_list),
        path.as_deref(),
        repo.as_deref(),
    );
    doc.body = scaffold::body(kind, title);

    let mut notes = vec![];
    if let Some(note) = derive_repo(ctx, &mut doc, path.as_deref()) {
        notes.push(note);
    }
    write_mint(
        ctx,
        Mint {
            kind,
            doc,
            new_id,
            path: note_path,
            notes,
        },
    )
}

/// Adopt an existing markdown file into the vault.
///
/// The file's body is kept verbatim below a frontmatter block built from the
/// schema table — an adopted file that brought its own block drops it, because
/// frontmatter is the tool's. Its title is the file's first `# ` heading outside
/// a code fence; with none, it is the filename, and `# <title>` is written above
/// the kept body so the note carries the title its id was minted from.
///
/// A stray file of this vault — a `.md` in a governed folder whose name is no
/// id, which `MX401` reports — is moved rather than copied, because a copy would
/// leave the finding it was adopted to repair. Any other source is left where it
/// was, and the outcome says which happened.
///
/// # Errors
/// Refuses the same flag problems [`new`] does, plus a source that cannot be
/// read or has nothing to slugify into an id.
pub fn adopt(ctx: Ctx<'_>, kind: Kind, source: &Path, args: NewArgs<'_>) -> Result<Outcome> {
    validate_new_args(kind, args)?;
    let refs = if args.refs.is_empty() {
        None
    } else {
        Some(refs_list(args.refs)?)
    };
    let index = ctx.index()?;
    let project = project_link(&index, kind, args.project)?;

    let src = std::fs::read_to_string(source).map_err(|e| Error::io(source, e))?;
    // A block that does not parse is still frontmatter, and still dropped; with
    // no closed block the whole file is the body.
    let body = frontmatter::body_after_block(&src).to_owned();
    let heading = markdown::first_heading(body.lines());
    let title = heading
        .clone()
        .or_else(|| {
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "untitled".to_owned());
    let slug = slug_of(&title)?;
    let new_id = format!("{}-{slug}", ctx.stamp);
    let note_path = ctx.root.join(kind.folder()).join(format!("{new_id}.md"));
    if note_path.exists() {
        return Err(Error::AlreadyExists(note_path));
    }
    ensure_id_free(ctx.root, &new_id, None)?;

    let stray =
        crate::vault::governed_file(ctx.root, source).is_some_and(|f| !id::is_valid(&f.stem));

    let mut notes = vec![];
    let mut doc = fields_for(
        kind,
        project.as_deref(),
        args.status,
        args.tags,
        refs.as_ref().and_then(Value::as_list),
        None,
        None,
    );
    doc.body = if heading.is_some() {
        body
    } else {
        notes.push(format!(
            "the body had no `# ` heading, so `# {title}` was written above it"
        ));
        let gap = if body.starts_with('\n') { "" } else { "\n" };
        format!("\n# {title}\n{gap}{body}")
    };
    let mut out = write_mint(
        ctx,
        Mint {
            kind,
            doc,
            new_id,
            path: note_path,
            notes,
        },
    )?;
    // The note is written and valid whatever happens to the source, so a source
    // that cannot be removed is reported, not raised.
    out.notes.push(if !stray {
        format!("{} was left where it was", source.display())
    } else if let Err(e) = std::fs::remove_file(source) {
        format!(
            "{} could not be moved, so it was copied: {e}",
            source.display()
        )
    } else {
        format!("moved from {}", source.display())
    });
    Ok(out)
}

// --- set ----------------------------------------------------------------

/// Set a field. Spec-driven: it covers every field the schema has, and gains a
/// field the moment the table does.
///
/// # Errors
/// Refuses a tool-owned field, an unknown field, a wrong arity, an off-enum
/// value, a `|` in a wiki-link, a `project` naming something that is not a
/// project, a bad `refs` entry, `--clear` on a required field, and frontmatter
/// it cannot rewrite.
pub fn set(
    ctx: Ctx<'_>,
    note: &str,
    field: &str,
    values: &[String],
    clear: bool,
) -> Result<Outcome> {
    let index = ctx.index()?;
    let subject = note_of(&index, note)?;
    let kind = subject.kind;

    if field == "id" {
        return Err(Error::IdIsNotAField);
    }
    let spec = kind.spec().field(field).ok_or_else(|| Error::NoSuchField {
        kind: kind.name(),
        field: field.to_owned(),
        settable: settable_fields(kind),
    })?;
    let mut doc = writable(subject)?;

    if clear {
        if spec.required {
            return Err(Error::RequiredCannotBeCleared(spec.name));
        }
        doc.remove(spec.name);
    } else {
        match spec.shape {
            Shape::TextList => doc.set(spec.name, list_value(note, spec.name, values)?),
            Shape::UrlList => {
                if values.is_empty() {
                    return Err(Error::ListNeedsAValue {
                        note: note.to_owned(),
                        field: spec.name,
                    });
                }
                doc.set(spec.name, refs_list(values)?);
            }
            _ => {
                let [value] = values else {
                    return Err(Error::TakesOneValue {
                        field: spec.name,
                        count: values.len(),
                    });
                };
                let stored = match spec.shape {
                    Shape::Enum(allowed) => {
                        if !allowed.contains(&value.as_str()) {
                            return Err(Error::OffEnum {
                                found: value.clone(),
                                field: spec.name,
                                values: allowed,
                            });
                        }
                        value.clone()
                    }
                    // A value carrying a `|` is refused rather than written for
                    // MX200 to find later.
                    Shape::WikiLink if value.contains('|') => {
                        return Err(Error::NotAWikiLink(value.clone()));
                    }
                    Shape::WikiLink => {
                        let target = expect_kind(&index, value, Kind::Project)?;
                        links::wrap(&target.id)
                    }
                    Shape::Remote => canonical_remote(value)?,
                    Shape::Text if spec.name == "path" => path::stored(value, ctx.home),
                    Shape::Text => value.clone(),
                    Shape::TextList | Shape::UrlList => unreachable!(),
                };
                doc.set(spec.name, Value::Scalar(stored));
            }
        }
    }

    let mut notes = vec![];
    if kind == Kind::Project && spec.name == "path" && !clear {
        let stored = doc
            .get("path")
            .and_then(|f| f.value.as_scalar())
            .map(ToOwned::to_owned);
        if let Some(derived) = derive_repo(ctx, &mut doc, stored.as_deref()) {
            notes.push(derived);
        }
    }

    save(&subject.path, kind, &doc)?;
    outcome(
        ctx,
        format!("set `{}` on {note}", spec.name),
        subject.path.clone(),
        notes,
    )
}

/// A text list's new value. An empty list is what `--clear` is for, and a blank
/// entry is no value, so both are refused.
fn list_value(note: &str, field: &'static str, values: &[String]) -> Result<Value> {
    if values.is_empty() {
        return Err(Error::ListNeedsAValue {
            note: note.to_owned(),
            field,
        });
    }
    if values.iter().any(|v| v.trim().is_empty()) {
        return Err(Error::BlankListValue(field));
    }
    Ok(Value::List(values.to_vec()))
}

/// The fields `set` covers: every field the kind's schema has.
#[must_use]
pub fn settable_fields(kind: Kind) -> Vec<&'static str> {
    kind.spec().fields.iter().map(|f| f.name).collect()
}

/// The canonical form of a remote, or the refusal that names the alternative.
fn canonical_remote(value: &str) -> Result<String> {
    crate::git::canonical(value).ok_or_else(|| Error::NotARemote(value.to_owned()))
}

/// Fill in `repo` from the checkout `path` names.
///
/// Only ever fills a **blank**: a stored `repo` is a claim, and a claim that
/// disagrees with the checkout is `MX404`'s to report, not this verb's to
/// overwrite. A directory that is no checkout — or whose `origin` is a local
/// path — derives nothing at all, which is silence rather than a finding.
///
/// Returns what to report, because a field written by inference must be a field
/// the writer is told about.
fn derive_repo(ctx: Ctx<'_>, doc: &mut Document, path: Option<&str>) -> Option<String> {
    if doc.get("repo").is_some_and(|f| !f.value.is_empty_scalar()) {
        return None;
    }
    let dir = path::resolve(ctx.root, ctx.home, path?)?;
    let origin = crate::git::origin(&dir)?;
    doc.set("repo", Value::Scalar(origin.clone()));
    Some(format!(
        "`repo` is `{origin}`, from the checkout at `{}`",
        dir.display()
    ))
}

/// `path` with `..` and symlinks resolved, or as given when it cannot be.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// --- rename / delete ----------------------------------------------------

/// Rewrite the first `# ` heading of a body. A `# ` line inside a fenced code
/// block is code, not a heading, and is left alone.
fn retitle(body: &str, title: &str) -> String {
    let mut out = String::with_capacity(body.len() + title.len());
    let mut done = false;
    let mut fences = Fences::default();
    for line in body.split_inclusive('\n') {
        // Every line is fed, so the fences stay tracked after the rewrite.
        if fences.is_prose(line) && !done && markdown::heading(line).is_some() {
            let _ = writeln!(out, "# {title}");
            done = true;
            continue;
        }
        out.push_str(line);
    }
    out
}

/// Rewrite every body link whose target is `old` exactly, to `new`.
///
/// A link is `[[old]]`, `[[old|label]]`, or `[[old#heading]]`: the target ends
/// where the id ends, so `[[old-extra]]` is left alone, and a label or heading
/// form must still close with `]]`. Text inside a fenced code block or an
/// inline code span is literal and is never rewritten. An inline code span
/// opens on a backtick run and closes on the next run of exactly the same
/// length; a blank line ends the paragraph, and a backtick preceded by an odd
/// number of backslashes is escaped, so neither delimits a span. A run with no
/// such partner is literal, so a link after it is still rewritten.
///
/// Returns the rewritten body and how many links were rewritten.
fn rewrite_body_links(body: &str, old: &str, new: &str) -> (String, usize) {
    let open = format!("[[{old}");
    let new_open = format!("[[{new}");
    let literal = literal_ranges(body);

    let mut out = String::with_capacity(body.len());
    let mut count = 0;
    let mut i = 0;
    let bytes = body.as_bytes();
    let mut ranges = literal.into_iter().peekable();

    while i < bytes.len() {
        while let Some(&(_, end)) = ranges.peek() {
            if end <= i {
                ranges.next();
            } else {
                break;
            }
        }
        if let Some(&(start, end)) = ranges.peek()
            && start <= i
            && i < end
        {
            out.push_str(&body[i..end]);
            i = end;
            continue;
        }

        // Prose. Match the id exactly, with a boundary after it.
        let rest = &body[i..];
        if let Some(after) = rest.strip_prefix(&open) {
            if after.starts_with("]]") {
                out.push_str(&new_open);
                out.push_str("]]");
                i += open.len() + 2;
                count += 1;
                continue;
            }
            if let Some(tail) = after.strip_prefix('|')
                && link_closes_on_line(tail)
            {
                out.push_str(&new_open);
                out.push('|');
                i += open.len() + 1;
                count += 1;
                continue;
            }
            if let Some(tail) = after.strip_prefix('#')
                && link_closes_on_line(tail)
            {
                out.push_str(&new_open);
                out.push('#');
                i += open.len() + 1;
                count += 1;
                continue;
            }
        }
        let ch = rest.chars().next().expect("i < len");
        out.push(ch);
        i += ch.len_utf8();
    }

    (out, count)
}

/// Whether the text after a link target's `|` or `#` closes its own link on
/// this line: the first `]]` appears before any newline, and no `[[` opens
/// another link between the boundary and that `]]`.
fn link_closes_on_line(tail: &str) -> bool {
    let line_end = tail.find('\n').unwrap_or(tail.len());
    let line = &tail[..line_end];
    let Some(close) = line.find("]]") else {
        return false;
    };
    !line[..close].contains("[[")
}

/// The byte ranges of a body that are literal: fenced code blocks and inline
/// code spans. A blank line ends a paragraph, so inline scanning does not cross
/// one. The ranges are disjoint and sorted.
fn literal_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = vec![];
    let mut fences = Fences::default();
    let mut prose_start: Option<usize> = None;
    let mut offset = 0;

    for line in body.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let prose = fences.is_prose(line);
        if prose && !is_blank_line(line) {
            if prose_start.is_none() {
                prose_start = Some(start);
            }
        } else {
            if !prose {
                ranges.push((start, offset));
            }
            if let Some(region_start) = prose_start.take() {
                for (s, e) in inline_code_ranges(&body[region_start..start]) {
                    ranges.push((region_start + s, region_start + e));
                }
            }
        }
    }
    if let Some(region_start) = prose_start {
        for (s, e) in inline_code_ranges(&body[region_start..]) {
            ranges.push((region_start + s, region_start + e));
        }
    }
    ranges.sort_unstable();
    ranges
}

/// Whether `line` is empty or spaces/tabs only (plus its newline), and so ends
/// a paragraph. Unicode whitespace such as a non-breaking space is not a blank
/// line in `CommonMark`, so it does not end one here.
fn is_blank_line(line: &str) -> bool {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    line.bytes().all(|b| matches!(b, b' ' | b'\t'))
}

/// The byte ranges of inline code spans in one prose region, relative to its
/// start. A span opens on an unescaped backtick run and closes on the next run
/// of exactly the same length; a run with no partner is literal and opens
/// nothing.
fn inline_code_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = vec![];
    let mut i = 0;
    while i < text.len() {
        let Some((start, end, run_len)) = backtick_run(text, i) else {
            break;
        };
        if let Some((_, close_end)) = matching_backtick_run(text, end, run_len) {
            ranges.push((start, close_end));
            i = close_end;
        } else {
            // No partner: the run is literal, so a link after it is prose.
            i = end;
        }
    }
    ranges
}

/// The first unescaped backtick run at or after `from`, if there is one: its
/// byte range and its length. A backtick preceded by an odd number of
/// backslashes is escaped and is skipped as literal text.
fn backtick_run(line: &str, from: usize) -> Option<(usize, usize, usize)> {
    let bytes = line.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            if is_escaped(line, i) {
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && bytes[i] == b'`' {
                i += 1;
            }
            return Some((start, i, i - start));
        }
        i += 1;
    }
    None
}

/// Whether the backtick at `i` is escaped by an odd number of backslashes, and
/// so is literal text rather than a code-span delimiter.
fn is_escaped(text: &str, i: usize) -> bool {
    let bytes = text.as_bytes();
    let mut backslashes = 0;
    let mut j = i;
    while j > 0 && bytes[j - 1] == b'\\' {
        backslashes += 1;
        j -= 1;
    }
    backslashes % 2 == 1
}

/// The run that closes an inline code span opened by a run of `open_len`
/// backticks: the next run of exactly that length, at or after `from`.
fn matching_backtick_run(line: &str, from: usize, open_len: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while let Some((start, end, len)) = backtick_run(line, i) {
        if len == open_len {
            return Some((start, end));
        }
        i = end;
    }
    None
}

/// Retitle a note: a new slug on the existing timestamp, the body heading, the
/// file, every spoke `project` field that names a renamed project, and every
/// body link whose target is the old id.
///
/// A body link is rewritten only when its target is the old id exactly, and
/// text inside a code fence or an inline code span is left alone. A governed
/// file whose frontmatter cannot be parsed or cannot be rendered without losing
/// a value is left untouched and named in the outcome, because its link cannot
/// be rewritten safely.
///
/// # Errors
/// Refuses a new path that exists. The operation is atomic in effect: if any
/// rewrite or the move itself fails, nothing is left moved.
pub fn rename(ctx: Ctx<'_>, note: &str, title: &str) -> Result<Outcome> {
    let index = ctx.index()?;
    let subject = note_of(&index, note)?;
    let kind = subject.kind;
    let mut doc = writable(subject)?;

    let slug = slug_of(title)?;
    // The note's existing timestamp: creation time does not change.
    let stamp = id::stamp(note).unwrap_or(ctx.stamp);
    let new_id = format!("{stamp}-{slug}");
    let new_path = subject.path.with_file_name(format!("{new_id}.md"));
    if new_id != note && new_path.exists() {
        return Err(Error::AlreadyExists(new_path));
    }
    ensure_id_free(ctx.root, &new_id, Some(&subject.path))?;

    doc.body = retitle(&doc.body, title);
    // The renamed note's own body is rewritten too, so a self-link moves with it.
    let own_body_links = if new_id == note {
        0
    } else {
        let (body, n) = rewrite_body_links(&doc.body, note, &new_id);
        doc.body = body;
        n
    };

    // Plan every write before performing any of them, and keep what each file
    // held, so a failure part-way can put everything back.
    let inbound = if new_id == note {
        Inbound::default()
    } else {
        plan_rewrites(ctx.root, &index, note, &new_id, kind, &subject.path)?
    };

    let mut written: Vec<(PathBuf, String)> = vec![];
    let restore = |written: &[(PathBuf, String)]| {
        for (p, original) in written {
            let _ = write_atomically(p, original);
        }
    };
    for (path, contents) in &inbound.planned {
        let original = std::fs::read_to_string(path).map_err(|e| {
            restore(&written);
            Error::io(path, e)
        })?;
        if let Err(e) = write_atomically(path, contents) {
            restore(&written);
            return Err(e);
        }
        written.push((path.clone(), original));
    }

    if let Err(e) = write_atomically(&new_path, &doc.render(kind)) {
        restore(&written);
        return Err(e);
    }
    if new_path != subject.path
        && let Err(e) = std::fs::remove_file(&subject.path)
    {
        let _ = std::fs::remove_file(&new_path);
        restore(&written);
        return Err(Error::io(&subject.path, e));
    }

    let mut notes = vec![];
    let updated = inbound.updated;
    if updated > 0 {
        notes.push(format!(
            "{updated} link{} updated",
            if updated == 1 { "" } else { "s" }
        ));
    }
    let body_links = inbound.body_links + own_body_links;
    if body_links > 0 {
        notes.push(format!(
            "{body_links} body link{} rewritten",
            if body_links == 1 { "" } else { "s" }
        ));
    }
    notes.extend(inbound.skipped);

    outcome(ctx, format!("renamed {note} to {new_id}"), new_path, notes)
}

/// What [`rename`] would do, without doing any of it: the same validations and
/// the same link-rewrite plan, reported rather than written.
///
/// # Errors
/// Refuses exactly what [`rename`] refuses — a title with no slug, a target that
/// exists, and a cross-folder id collision.
pub fn rename_preview(ctx: Ctx<'_>, note: &str, title: &str) -> Result<Outcome> {
    let index = ctx.index()?;
    let subject = note_of(&index, note)?;
    let mut doc = writable(subject)?;

    let slug = slug_of(title)?;
    let stamp = id::stamp(note).unwrap_or(ctx.stamp);
    let new_id = format!("{stamp}-{slug}");
    let new_path = subject.path.with_file_name(format!("{new_id}.md"));
    if new_id != note && new_path.exists() {
        return Err(Error::AlreadyExists(new_path));
    }
    ensure_id_free(ctx.root, &new_id, Some(&subject.path))?;

    doc.body = retitle(&doc.body, title);
    let own_body_links = if new_id == note {
        0
    } else {
        rewrite_body_links(&doc.body, note, &new_id).1
    };

    let inbound = if new_id == note {
        Inbound::default()
    } else {
        plan_rewrites(ctx.root, &index, note, &new_id, subject.kind, &subject.path)?
    };

    let mut notes = vec![format!("would rename {note} to {new_id}")];
    let updated = inbound.updated;
    if updated > 0 {
        notes.push(format!(
            "{updated} link{} would be updated",
            if updated == 1 { "" } else { "s" }
        ));
    }
    let body_links = inbound.body_links + own_body_links;
    if body_links > 0 {
        notes.push(format!(
            "{body_links} body link{} would be rewritten",
            if body_links == 1 { "" } else { "s" }
        ));
    }
    notes.extend(inbound.skipped);
    Ok(Outcome {
        headline: format!("would rename {note} to {new_id}"),
        path: new_path,
        notes,
        diagnostics: vec![],
    })
}

/// What renaming a note does to the links that name it: every spoke `project`
/// field and every body link, planned as one write per file.
#[derive(Default)]
struct Inbound {
    /// Each rewritten file: its path and its new contents.
    planned: Vec<(PathBuf, String)>,
    /// How many spoke `project` links the planned rewrites change.
    updated: usize,
    /// How many body link occurrences the planned rewrites change.
    body_links: usize,
    /// Advice naming each file left as it is, because it cannot be rewritten
    /// safely.
    skipped: Vec<String>,
}

/// Every link that names `old`, planned as one write per file: spokes whose
/// `project` field names it, and bodies whose prose links it. A file that needs
/// both is planned once.
///
/// A governed file whose frontmatter cannot be parsed, or cannot be rendered
/// without losing a value, is never rewritten — the same refusal `set` gives —
/// and is named in [`Inbound::skipped`] instead.
fn plan_rewrites(
    root: &Path,
    index: &Index,
    old: &str,
    new: &str,
    kind: Kind,
    subject: &Path,
) -> Result<Inbound> {
    let mut inbound = Inbound::default();
    let mut planned: BTreeMap<PathBuf, (Kind, Document)> = BTreeMap::new();

    if kind == Kind::Project {
        for other in index.spokes_of(old) {
            let mut doc = other.doc.clone();
            let Some(field) = doc.get_mut("project") else {
                continue;
            };
            let Value::Scalar(s) = &mut field.value else {
                continue;
            };
            if links::target(s) != Some(old) {
                continue;
            }
            if !other.doc.is_renderable() {
                inbound.skipped.push(format!(
                    "{} names `{old}` in `project` but has frontmatter this tool cannot rewrite without losing it; update that link by hand",
                    other.path.display()
                ));
                continue;
            }
            *s = links::wrap(new);
            planned.insert(other.path.clone(), (other.kind, doc));
            inbound.updated += 1;
        }
    }

    if new != old {
        for file in crate::vault::governed_files(root)? {
            if file.path == subject {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&file.path) else {
                continue;
            };
            let Ok(mut doc) = frontmatter::parse(&src) else {
                let (_, n) = rewrite_body_links(&src, old, new);
                if n > 0 {
                    inbound.skipped.push(format!(
                        "{} still mentions `{old}` but its frontmatter cannot be parsed; update that link by hand",
                        file.path.display()
                    ));
                }
                continue;
            };
            if !doc.is_renderable() {
                let (_, n) = rewrite_body_links(&doc.body, old, new);
                if n > 0 {
                    inbound.skipped.push(format!(
                        "{} links `{old}` in its body but has frontmatter this tool cannot rewrite without losing it; update that link by hand",
                        file.path.display()
                    ));
                }
                continue;
            }
            let (body, n) = rewrite_body_links(&doc.body, old, new);
            if n == 0 {
                continue;
            }
            if let Some((_, existing)) = planned.get_mut(&file.path) {
                existing.body = body;
            } else {
                doc.body = body;
                planned.insert(file.path.clone(), (file.kind, doc));
            }
            inbound.body_links += n;
        }
    }

    for (path, (kind, doc)) in planned {
        inbound.planned.push((path, doc.render(kind)));
    }
    Ok(inbound)
}

/// Notes whose *body* still mentions an id, each with `reason` for why it
/// matters. `delete` uses this, advisory only, with no exit-code effect: a
/// deletion cannot rewrite a body link.
fn body_mentions(root: &Path, id: &str, reason: &str) -> Result<Vec<String>> {
    let needle = format!("[[{id}]]");
    let mut out = vec![];
    for file in crate::vault::governed_files(root)? {
        let Ok(src) = std::fs::read_to_string(&file.path) else {
            continue;
        };
        let body = frontmatter::parse(&src).map(|d| d.body).unwrap_or(src);
        if body.contains(&needle) {
            out.push(format!(
                "the body of {} still mentions `{id}`; {reason}",
                file.path.display()
            ));
        }
    }
    Ok(out)
}

/// Remove a note.
///
/// # Errors
/// Refuses a project that any spoke still names in `project`, listing each such
/// id and the verb that moves it. A spoke deletes outright: nothing else names
/// it. A body elsewhere that still links the id is reported after the removal,
/// advisory only — bodies are not rewritten.
pub fn delete(ctx: Ctx<'_>, note: &str) -> Result<Outcome> {
    let index = ctx.index()?;
    let subject = note_of(&index, note)?;
    if subject.kind == Kind::Project {
        let spokes: Vec<String> = index.spokes_of(note).iter().map(|n| n.id.clone()).collect();
        if !spokes.is_empty() {
            return Err(Error::DeleteWouldOrphan {
                id: note.to_owned(),
                spokes,
            });
        }
    }
    std::fs::remove_file(&subject.path).map_err(|e| Error::io(&subject.path, e))?;
    // Computed after the removal, so the note's own body is not counted.
    let notes = body_mentions(ctx.root, note, "that note no longer exists")?;
    Ok(Outcome {
        headline: format!("deleted {note}"),
        path: subject.path.clone(),
        notes,
        diagnostics: vec![],
    })
}
