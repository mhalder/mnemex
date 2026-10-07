//! A temp vault the tests can build up note by note.
#![allow(dead_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A vault in a temp directory, removed when it drops.
pub struct Vault {
    dir: TempDir,
}

impl Vault {
    /// An empty directory that is not yet a vault.
    #[must_use]
    pub fn bare() -> Self {
        Self {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }

    /// A vault with all eight governed folders.
    #[must_use]
    pub fn new() -> Self {
        let v = Self::bare();
        for k in mnemex::kind::Kind::ALL {
            std::fs::create_dir_all(v.root().join(k.folder())).expect("mkdir");
        }
        v
    }

    /// The vault root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Write `<folder>/<id>.md` with `contents`, returning its path.
    pub fn write(&self, folder: &str, id: &str, contents: &str) -> PathBuf {
        let dir = self.root().join(folder);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join(format!("{id}.md"));
        std::fs::write(&path, contents).expect("write");
        path
    }

    /// Write a file verbatim at `<folder>/<name>`.
    pub fn raw(&self, folder: &str, name: &str, contents: &str) -> PathBuf {
        let dir = self.root().join(folder);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join(name);
        std::fs::write(&path, contents).expect("write");
        path
    }

    /// Write a note with the given frontmatter body and an empty body.
    pub fn note(&self, folder: &str, id: &str, frontmatter: &str) -> PathBuf {
        self.write(folder, id, &format!("---\n{frontmatter}---\n\n# {id}\n"))
    }

    /// Create a directory inside the temp area (not necessarily in the vault).
    pub fn dir(&self, rel: &str) -> PathBuf {
        let p = self.root().join(rel);
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    /// A directory that is a git checkout whose `origin` is `url`.
    pub fn checkout(&self, rel: &str, url: &str) -> PathBuf {
        self.checkout_with(rel, &[("origin", url)])
    }

    /// A directory that is a git checkout with exactly `remotes`, in order.
    pub fn checkout_with(&self, rel: &str, remotes: &[(&str, &str)]) -> PathBuf {
        let dir = self.dir(rel);
        let git = self.dir(&format!("{rel}/.git"));
        let mut config = String::from("[core]\n\tbare = false\n");
        for (name, url) in remotes {
            let _ = write!(
                config,
                "[remote \"{name}\"]\n\turl = {url}\n\tfetch = +refs/heads/*:refs/remotes/{name}/*\n"
            );
        }
        std::fs::write(git.join("config"), config).expect("config");
        dir
    }
}

impl Default for Vault {
    fn default() -> Self {
        Self::new()
    }
}
