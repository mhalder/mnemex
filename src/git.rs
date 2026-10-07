//! The `git remote` shape, and where its value comes from.
//!
//! A project's `path` is machine-local — it is where the work happens *here* —
//! so it cannot say which repository a project is. `repo` is the other half:
//! the identity that survives a second machine, a second clone, and a moved
//! directory.
//!
//! Identity only works if two spellings of one repository compare equal, so the
//! stored form is canonical — `host/owner/name`, no scheme, no credentials, no
//! port, no `.git` — and the verbs canonicalise on the way in, exactly as
//! [`crate::links::wrap`] wraps a bare id. `MX108` is what catches a block that
//! was hand-written past them.
//!
//! A checkout may know its work by more than one remote — a fork kept beside an
//! upstream `origin`, a vault whose remotes name the layers it serves — so
//! [`remotes`] reads them all and `check` accepts a `repo` that is any of them.
//! The verbs that *derive* the field still name `origin` alone: choosing among
//! several is the writer's call, not this tool's.
//!
//! **The remote is read from `.git/config`, never from a `git` subprocess.**
//! `check --root` visits every project note and the write hook runs on every
//! write; one spawn per note would cost more than the whole run is allowed.
//! The cost is that `insteadOf` rewrites and conditional includes are not
//! honoured — a config this tool reads plainly.

use std::path::{Path, PathBuf};

/// The marker git leaves in a checkout: a directory, or a file naming one.
const DOT_GIT: &str = ".git";

/// Whether `host` can name a machine: a dotted name, or `localhost`.
///
/// A bare word is refused, which is what keeps `owner/name` from passing as a
/// remote whose host is `owner`, and keeps canonicalisation idempotent — a
/// value that would not survive being read back is never written.
pub(crate) fn is_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && (host.contains('.') || host == "localhost")
}

/// Whether `segment` is a usable path segment of a remote.
fn is_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'~' | b'+' | b'%')
        })
}

/// The schemes that name a remote machine. `file` is deliberately absent: a
/// local checkout is what `path` names, and it is no identity.
const SCHEMES: [&str; 4] = ["ssh", "git", "http", "https"];

/// Strip a scheme or an scp-like `host:path`, leaving `[user@]host[:port]/path`.
fn without_scheme(url: &str) -> Option<String> {
    if let Some((scheme, rest)) = url.split_once("://") {
        return SCHEMES
            .contains(&scheme.to_ascii_lowercase().as_str())
            .then(|| rest.to_owned());
    }
    if url.starts_with(['/', '.', '~']) {
        return None; // A local path, which `path` already names.
    }
    // scp-like `[user@]host:path`, which is a remote only while the colon comes
    // before any slash: `a/b:c` is a path with a colon in it.
    match url.split_once(':') {
        Some((head, tail)) if !head.contains('/') => Some(format!("{head}/{tail}")),
        Some(_) => None,
        None => Some(url.to_owned()),
    }
}

/// The canonical form of a remote URL, or `None` when it names no repository
/// this tool can identify.
///
/// Every spelling of one repository — ssh, https, scp-like, with or without
/// credentials, a port, a trailing `.git` or a trailing slash — canonicalises
/// to one value, and that value canonicalises to itself. The path must name at
/// least an owner and a name: `github.com/owner` is an account, not a
/// repository.
#[must_use]
pub fn canonical(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || url.chars().any(char::is_whitespace) {
        return None;
    }
    let rest = without_scheme(url)?;
    let (authority, path) = rest.split_once('/')?;
    // Credentials are not identity. `user@host` keeps the host, and credentials
    // can only come before it: an `@` past the first `/` belongs to the path.
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    // A port is where the machine listens, not which machine it is.
    let host = authority.split_once(':').map_or(authority, |(h, _)| h);
    let host = host.to_ascii_lowercase();
    if !is_host(&host) {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() < 2 || !segments.iter().all(|s| is_segment(s)) {
        return None;
    }
    Some(format!("{host}/{}", segments.join("/")))
}

/// Whether `value` is already the form a note stores: its own fixed point.
#[must_use]
pub fn is_canonical(value: &str) -> bool {
    canonical(value).as_deref() == Some(value)
}

/// The git directory of the checkout at `dir`: `.git`, or the directory a
/// `.git` **file** names — which is how a worktree and a submodule point at
/// theirs.
fn git_dir(dir: &Path) -> Option<PathBuf> {
    let dot = dir.join(DOT_GIT);
    if dot.is_dir() {
        return Some(dot);
    }
    let named = std::fs::read_to_string(&dot).ok()?.lines().find_map(|l| {
        l.trim()
            .strip_prefix("gitdir:")
            .map(|p| p.trim().to_owned())
    })?;
    Some(resolve_relative(dir, &named))
}

/// A git path that may be relative to the directory it was named in.
fn resolve_relative(base: &Path, named: &str) -> PathBuf {
    let path = Path::new(named);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// The config a git directory reads its remotes from. A linked worktree holds
/// no remotes of its own; `commondir` names the checkout whose remotes it uses.
fn config_of(git_dir: &Path) -> PathBuf {
    let common = std::fs::read_to_string(git_dir.join("commondir"))
        .ok()
        .and_then(|t| t.lines().next().map(|l| l.trim().to_owned()))
        .filter(|l| !l.is_empty());
    match common {
        Some(named) => resolve_relative(git_dir, &named).join("config"),
        None => git_dir.join("config"),
    }
}

/// The remote a section header names, however it is spaced: the subsection of
/// `[remote "name"]`. Any other section — `[core]`, `[branch "main"]` — names
/// no remote.
fn remote_name(section: &str) -> Option<&str> {
    section
        .trim()
        .strip_prefix("remote")?
        .trim_start()
        .strip_prefix('"')?
        .strip_suffix('"')
}

/// Every remote of `config` as `(name, url)`, in file order, each section's
/// first `url`.
fn remote_urls(config: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(config) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = Vec::new();
    let mut in_remote: Option<&str> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_remote = remote_name(section);
            continue;
        }
        let Some(name) = in_remote else {
            continue;
        };
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
            && !out.iter().any(|(seen, _)| seen == name)
        {
            out.push((name.to_owned(), value.trim().to_owned()));
        }
    }
    out
}

/// Whether `dir` is a git checkout.
#[must_use]
pub fn is_checkout(dir: &Path) -> bool {
    git_dir(dir).is_some()
}

/// The canonical identity of the checkout at `dir`, if it is one and its
/// `origin` names a repository.
///
/// Everything else — no checkout, no `origin`, an `origin` that is a local
/// path — is `None`. [`remotes`] is the wider question the layout rules ask.
#[must_use]
pub fn origin(dir: &Path) -> Option<String> {
    let config = config_of(&git_dir(dir)?);
    remote_urls(&config)
        .into_iter()
        .find(|(name, _)| name == "origin")
        .and_then(|(_, url)| canonical(&url))
}

/// The canonical identities of every remote of the checkout at `dir`, in
/// config order with duplicates dropped.
///
/// `check` compares a note's `repo` against all of them, because `origin` is
/// not the only name a checkout can know its work by. A remote this tool cannot
/// canonicalise — a local path, a bare word — is no identity, exactly as it is
/// not one for `origin`, and is left out rather than counted.
///
/// A directory that is no checkout has no remotes, which is silence rather than
/// a finding.
#[must_use]
pub fn remotes(dir: &Path) -> Vec<String> {
    let Some(git_dir) = git_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for (_, url) in remote_urls(&config_of(&git_dir)) {
        if let Some(identity) = canonical(&url)
            && !out.contains(&identity)
        {
            out.push(identity);
        }
    }
    out
}
