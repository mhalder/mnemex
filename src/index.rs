//! id → note, and a project's spokes.
//!
//! A note whose frontmatter will not parse, or that cannot be read, is **kept by
//! id only**: its id still names a note, so a link to it does not dangle, but it
//! has no frontmatter to offer — no fields, no links — and it reports its own
//! `MX001` when checked. A file whose stem is not a valid id is **not indexed at
//! all**: it has no id to be reached by, and it reports its own `MX401`.
//!
//! Lookups by id go through maps, so a whole-vault check that asks about every
//! link of every note stays linear in the vault's size.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::frontmatter::{self, Document};
use crate::id;
use crate::kind::Kind;
use crate::links;
use crate::vault::GovernedFile;

/// A note the index could read: a valid id, and frontmatter that parsed.
#[derive(Clone, Debug)]
pub struct Note {
    /// The filename stem, which is the note's identity.
    pub id: String,
    /// Its path.
    pub path: PathBuf,
    /// The kind its folder gives it.
    pub kind: Kind,
    /// Its frontmatter.
    pub doc: Document,
}

/// The vault, indexed.
#[derive(Clone, Debug)]
pub struct Index {
    /// The vault root.
    pub root: PathBuf,
    /// Every readable note, in index order.
    notes: Vec<Note>,
    /// Each id's position in `notes`. When an id names more than one note
    /// (`MX406`), the first in index order answers.
    by_id: HashMap<String, usize>,
    /// The kind of every note whose frontmatter could not be read, by id.
    unparseable: HashMap<String, Kind>,
    /// Project id → positions of every note whose `project` names it, in index
    /// order.
    spokes: HashMap<String, Vec<usize>>,
}

impl Index {
    /// Walk the governed folders and index what is reachable.
    ///
    /// # Errors
    /// Reports a governed folder that cannot be read.
    pub fn build(root: &Path) -> Result<Index> {
        let files = crate::vault::governed_files(root)?;
        Ok(Index::from_files(root, &files))
    }

    /// Index files a caller has already walked with
    /// [`crate::vault::governed_files`], so a caller that needs the walk as
    /// well walks the vault only once.
    #[must_use]
    pub fn from_files(root: &Path, files: &[GovernedFile]) -> Index {
        let mut notes = vec![];
        let mut by_id = HashMap::new();
        let mut unparseable = HashMap::new();
        for file in files {
            if !id::is_valid(&file.stem) {
                continue;
            }
            let doc = std::fs::read_to_string(&file.path)
                .ok()
                .and_then(|src| frontmatter::parse(&src).ok());
            match doc {
                Some(doc) => {
                    by_id.entry(file.stem.clone()).or_insert(notes.len());
                    notes.push(Note {
                        id: file.stem.clone(),
                        path: file.path.clone(),
                        kind: file.kind,
                        doc,
                    });
                }
                None => {
                    unparseable.entry(file.stem.clone()).or_insert(file.kind);
                }
            }
        }
        let mut spokes: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, note) in notes.iter().enumerate() {
            if let Some(project) = project_of(note) {
                spokes.entry(project.to_owned()).or_default().push(i);
            }
        }
        Index {
            root: root.to_path_buf(),
            notes,
            by_id,
            unparseable,
            spokes,
        }
    }

    /// Every note whose frontmatter could be read, in index order.
    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.notes.iter()
    }

    /// Whether the vault holds a note with this id, readable or not.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id) || self.unparseable.contains_key(id)
    }

    /// The note with this id, if its frontmatter could be read.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Note> {
        self.by_id.get(id).map(|&i| &self.notes[i])
    }

    /// The kind of the note with this id, readable or not.
    #[must_use]
    pub fn kind_of(&self, id: &str) -> Option<Kind> {
        self.get(id)
            .map(|n| n.kind)
            .or_else(|| self.unparseable.get(id).copied())
    }

    /// Every note whose `project` field names `project`, in index order.
    #[must_use]
    pub fn spokes_of(&self, project: &str) -> Vec<&Note> {
        self.spokes.get(project).map_or_else(Vec::new, |idx| {
            idx.iter().map(|&i| &self.notes[i]).collect()
        })
    }
}

/// The project a note's `project` field names, if it is a well-formed link.
fn project_of(note: &Note) -> Option<&str> {
    let field = note.doc.get("project")?;
    let text = field.value.as_scalar()?;
    links::target(text)
}
