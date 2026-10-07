//! The one vault root, and the governed walk.
//!
//! One vault: `$MEMEX_VAULT` if set and non-empty, else `$HOME/memex`. Nothing
//! validates that the directory "looks like a vault" — a verb that reads fails
//! when the directory does not exist, and `new`/`adopt` create it.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::kind::Kind;

/// One file directly inside a governed folder.
#[derive(Clone, Debug)]
pub struct GovernedFile {
    /// The kind its folder gives it.
    pub kind: Kind,
    /// Its path.
    pub path: PathBuf,
    /// Its filename stem, which is its id when the stem is a valid one.
    pub stem: String,
}

/// The one vault root: `$MEMEX_VAULT` if set and non-empty, else `$HOME/memex`.
///
/// # Errors
/// [`Error::NoVault`] when neither `$MEMEX_VAULT` nor `$HOME` names a vault.
pub fn root() -> Result<PathBuf> {
    root_from(
        std::env::var_os("MEMEX_VAULT").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// [`root`], with the two environment values passed in, so tests can drive it
/// without mutating the process environment.
#[doc(hidden)]
pub fn root_from(env: Option<&std::ffi::OsStr>, home: Option<&std::ffi::OsStr>) -> Result<PathBuf> {
    if let Some(env) = env.filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(env));
    }
    match home.filter(|p| !p.is_empty()) {
        Some(home) => Ok(PathBuf::from(home).join("memex")),
        None => Err(Error::NoVault),
    }
}

/// Every `*.md` directly inside a governed folder, in index order: kind in
/// vault-layout order, then filename. Governed folders are flat; the walk does
/// not recurse.
///
/// # Errors
/// Reports a governed folder that exists but cannot be read.
pub fn governed_files(root: &Path) -> Result<Vec<GovernedFile>> {
    let mut out = vec![];
    for kind in Kind::ALL {
        let dir = root.join(kind.folder());
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(Error::io(dir, e)),
        };
        let mut batch = vec![];
        for entry in entries {
            let entry = entry.map_err(|e| Error::io(dir.clone(), e))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            batch.extend(governed_entry(kind, path));
        }
        batch.sort_by(|a, b| a.path.cmp(&b.path));
        out.append(&mut batch);
    }
    Ok(out)
}

/// The governed file `path` names in the vault at `root`, by the rule
/// [`governed_files`] walks with: a `.md` file directly inside one of `root`'s
/// governed folders.
///
/// The folder and the root are compared canonically, so `..` and symlinks do
/// not change the answer. The file itself need not exist.
#[must_use]
pub fn governed_file(root: &Path, path: &Path) -> Option<GovernedFile> {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let folder = canonical(path.parent()?);
    let kind = Kind::from_folder(folder.file_name()?.to_str()?)?;
    if folder.parent() != Some(canonical(root).as_path()) {
        return None;
    }
    governed_entry(kind, folder.join(path.file_name()?))
}

/// `path` as a governed file of `kind`'s folder, if it is a `.md` file with a
/// stem that can be an id at all.
fn governed_entry(kind: Kind, path: PathBuf) -> Option<GovernedFile> {
    if path.extension().is_none_or(|e| e != "md") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.to_owned();
    Some(GovernedFile { kind, path, stem })
}
