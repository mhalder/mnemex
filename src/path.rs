//! A project's `path` field: resolution and `~/`-relative storage.
//!
//! The stored form is portable across machines: a path inside `$HOME` is kept
//! as `~/<rest>` no matter how it was spelled, so the same vault can live on
//! several machines.

use std::path::{Path, PathBuf};

/// Resolve a project's stored `path` field to a directory.
///
/// `~/` and `$HOME/` expand; an absolute path is used as-is; a **relative** path
/// resolves against the vault root, which is what lets a fixture vault carry its
/// own workspace; a blank field names nothing.
#[must_use]
pub fn resolve(root: &Path, home: Option<&Path>, field: &str) -> Option<PathBuf> {
    let field = field.trim();
    if field.is_empty() {
        return None;
    }
    for prefix in ["~/", "$HOME/"] {
        if let Some(rest) = field.strip_prefix(prefix) {
            return Some(home?.join(rest));
        }
    }
    let path = Path::new(field);
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        Some(root.join(path))
    }
}

/// The stored spelling of a `path`: `~/<rest>` when it is inside `$HOME`,
/// otherwise as given.
///
/// The CLI resolves relative spellings against the working directory before
/// this runs, so a relative value arriving here is kept as-is and resolves
/// against the vault root when read.
#[must_use]
pub fn stored(value: &str, home: Option<&Path>) -> String {
    let value = value.trim();
    if value.is_empty() {
        return value.to_owned();
    }
    if let Some(rest) = value.strip_prefix("$HOME/") {
        return format!("~/{rest}");
    }
    if value.starts_with("~/") {
        return value.to_owned();
    }
    let path = Path::new(value);
    if path.is_absolute()
        && let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        return format!("~/{}", rest.display());
    }
    value.to_owned()
}
