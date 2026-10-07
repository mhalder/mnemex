//! Typed refusals and failures. Every one is a variant with its own `Display`,
//! which is what makes their messages testable strings rather than prose. A
//! refusal names the alternative wherever there is one; a failure such as an
//! I/O error names what failed and why.

use std::path::PathBuf;

/// Anything the library refuses or cannot do.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Neither `$MNEMEX_VAULT` nor `$HOME` names a vault.
    #[error("no vault: set `MNEMEX_VAULT`, or a `$HOME` directory to hold `~/mnemex-vault`")]
    NoVault,
    /// The one vault does not exist, and the verb reads rather than creates.
    #[error("no vault at `{}`; set `MNEMEX_VAULT` or create `~/mnemex-vault`", .path.display())]
    NoVaultAt {
        /// The directory that should be a vault.
        path: PathBuf,
    },
    /// `--install`/`--check` without a directory, and no environment names one.
    #[error("no default directory; pass one with `--install <DIR>` or `--check <DIR>`")]
    NoInstallDirectory,
    /// A path that is not a note of the vault it was checked against.
    #[error(
        "{path} is not a note of the vault at {root}; a note is a `.md` file directly inside one of its governed folders"
    )]
    NotANote {
        /// The path given.
        path: PathBuf,
        /// The vault it was checked against.
        root: PathBuf,
    },
    // --- refusals. Each names the alternative where there is one. ---
    /// A non-project kind created without `--project`.
    #[error("{} {kind} needs `--project <project-id>`", article(.kind))]
    ProjectRequired {
        /// The kind.
        kind: &'static str,
    },
    /// A project given `--project`.
    #[error("a project has no `project` field, so `--project` does not apply")]
    ProjectNotApplicable,
    /// `--status` on a kind whose status the caller cannot name here.
    #[error("`--status` does not apply to {} {kind}", article(.kind))]
    StatusNotApplicable {
        /// The kind.
        kind: &'static str,
    },
    /// `--ref` on a kind with no `refs`.
    #[error("`--ref` does not apply to {} {kind}", article(.kind))]
    RefsNotApplicable {
        /// The kind.
        kind: &'static str,
    },
    /// A value that is not a work-item URL.
    #[error(
        "`{0}` is not a work-item URL: a ref is an `http://` or `https://` URL with a path and no credentials, or a bare `host/path` taken as `https://host/path`, as in `https://github.com/owner/name/pull/42`"
    )]
    NotAUrl(String),
    /// A kind name that names no kind.
    #[error("`{found}` is not a kind; choose one of {}", listed(.kinds))]
    UnknownKind {
        /// What was given.
        found: String,
        /// Every kind's name.
        kinds: Vec<&'static str>,
    },
    /// `--path` on any kind but project.
    #[error("only a project has a `path`, so `--path` does not apply to {} {kind}", article(.kind))]
    PathNotApplicable {
        /// The kind.
        kind: &'static str,
    },
    /// `--repo` on any kind but project.
    #[error("only a project has a `repo`, so `--repo` does not apply to {} {kind}", article(.kind))]
    RepoNotApplicable {
        /// The kind.
        kind: &'static str,
    },
    /// A remote with no host, or one this tool cannot canonicalise. Refused
    /// rather than written for `MX108` to find later.
    #[error(
        "`{0}` is not a git remote: it needs a host, an owner and a name, as in \
         `git@github.com:owner/name.git` — a checkout on this machine is what `path` names"
    )]
    NotARemote(String),
    /// A file is already there. No overwrite, and no silent mutation of the
    /// slug or timestamp to dodge the collision.
    #[error("{0} already exists")]
    AlreadyExists(PathBuf),
    /// A minted id already names a note in another governed folder.
    #[error(
        "`{id}` already names a note in `{folder}/`; ids are unique across the vault, so choose a different title"
    )]
    DuplicateId {
        /// The id that would be minted.
        id: String,
        /// The folder of the note that already has it.
        folder: &'static str,
    },
    /// A title that slugifies to nothing.
    #[error("`{0}` has no letters or digits to slugify, so it cannot become an id")]
    TitleHasNoSlug(String),
    /// A title holding a newline, a tab or another control character.
    #[error(
        "`{}` holds a control character; a title is one line of text, so remove it",
        .0.escape_debug()
    )]
    TitleHasControlCharacter(String),
    /// A blank value for a list.
    #[error("`{0}` cannot hold a blank value; remove the empty one")]
    BlankListValue(&'static str),
    /// `set` on a list with no values.
    #[error("`{field}` needs at least one value; `mnemex set {note} {field} --clear` removes it")]
    ListNeedsAValue {
        /// The note.
        note: String,
        /// The list field.
        field: &'static str,
    },
    /// An id that names nothing.
    #[error("`{0}` is no note in this vault")]
    NoSuchNote(String),
    /// A note that exists but whose frontmatter does not parse, asked about by
    /// a query that needs its links.
    #[error(
        "`{id}` is a note at {}, but its frontmatter does not parse, so its links cannot be shown; \
         `mnemex check --path {}` says why",
        .path.display(),
        .path.display()
    )]
    NoteDoesNotParse {
        /// The id asked about.
        id: String,
        /// Where the note is.
        path: PathBuf,
    },
    /// A note of the wrong kind, named with the folder it is actually in.
    #[error("`{id}` is {} {found} in `{folder}/`, not a {wanted}", article(.found))]
    WrongKind {
        /// The note.
        id: String,
        /// The kind it is.
        found: &'static str,
        /// The folder it is in.
        folder: &'static str,
        /// The kind the verb needed.
        wanted: &'static str,
    },
    /// `set id`.
    #[error("`id` is not a field: a note's id is its filename — use `mnemex rename`")]
    IdIsNotAField,
    /// A field this kind does not have.
    #[error("{} `{kind}` has no `{field}`; its fields are {}", article(.kind), listed(.settable))]
    NoSuchField {
        /// The kind.
        kind: &'static str,
        /// What was asked for.
        field: String,
        /// The fields `set` covers.
        settable: Vec<&'static str>,
    },
    /// More than one value for a scalar.
    #[error("`{field}` takes one value, not {count}")]
    TakesOneValue {
        /// The field.
        field: &'static str,
        /// How many values were given.
        count: usize,
    },
    /// A value outside a closed enum.
    #[error("`{found}` is not a `{field}`; choose one of {}", listed(.values))]
    OffEnum {
        /// What was given.
        found: String,
        /// The field.
        field: &'static str,
        /// The values the enum allows.
        values: &'static [&'static str],
    },
    /// `--clear` on a required field.
    #[error("`{0}` is required, so it cannot be cleared")]
    RequiredCannotBeCleared(&'static str),
    /// A `|` in a wiki-link value.
    #[error("`{0}` is not a wiki-link: a link is `[[id]]`, with no display text")]
    NotAWikiLink(String),
    /// Frontmatter carrying a value the flat model cannot hold, or a key stated
    /// twice. Every writing verb refuses to rewrite it rather than destroy data.
    #[error("{0} has frontmatter this tool cannot rewrite without losing it")]
    Unrenderable(PathBuf),
    /// A project that spokes still name.
    #[error("{}", orphaned(.id, .spokes))]
    DeleteWouldOrphan {
        /// The project.
        id: String,
        /// The spokes naming it.
        spokes: Vec<String>,
    },
    /// `brief` was asked about a directory no single project owns.
    #[error("{0}")]
    NoProjectForBrief(String),
    /// The filesystem said no.
    #[error("{path}: {source}")]
    Io {
        /// What was being read or written.
        path: PathBuf,
        /// Why it failed.
        #[source]
        source: std::io::Error,
    },
}

/// `` `a` ``, `` `b` `` — the way every refusal lists a closed set.
fn listed<S: AsRef<str>>(values: &[S]) -> String {
    values
        .iter()
        .map(|v| format!("`{}`", v.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The indefinite article for a kind name. `adr` is the only vowel.
fn article(kind: &str) -> &'static str {
    if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// The project a spoke names cannot be deleted while the spoke does.
fn orphaned(id: &str, spokes: &[String]) -> String {
    format!(
        "`{id}` is named in `project` by {}; `mnemex set <spoke> project <other>` moves each, or delete it first",
        listed(spokes)
    )
}

impl Error {
    /// An [`Error::Io`] naming the path it happened to.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

/// The library's result type.
pub type Result<T> = std::result::Result<T, Error>;
