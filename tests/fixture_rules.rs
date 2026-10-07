//! Fixture-driven rules.
//!
//! Every rule owns `tests/fixtures/<code>/{good,bad}/`. Each polarity is a small
//! vault, so cross-file rules get supporting notes, and the claim is about the
//! vault: `bad/` must raise the rule somewhere, `good/` nowhere. The harness
//! also asserts that every rule directory has both polarities, and that every
//! rule has a directory.
//!
//! **A fixture that needs a checkout commits `dot-git` instead of `.git`**, and
//! the harness copies the vault to a temp directory and renames it before
//! checking. Git refuses to track any path whose component is `.git`, and a
//! stray one left in the working tree would make its directory look like a
//! nested repository — so materialising it away from the working tree is the
//! only form the rule can take.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mnemex::check::{self, Env};
use mnemex::diagnostic::Code;
use tempfile::TempDir;

/// What a fixture is checked as: the committed directory, or a temp copy that
/// holds the `.git` the committed one cannot.
enum Prepared {
    Committed(PathBuf),
    Materialised(TempDir),
}

impl Prepared {
    fn path(&self) -> &Path {
        match self {
            Prepared::Committed(p) => p,
            Prepared::Materialised(t) => t.path(),
        }
    }
}

/// The name a fixture commits a `.git` under.
const DOT_GIT: &str = "dot-git";

fn copy_into(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        let target = if name == DOT_GIT {
            to.join(".git")
        } else {
            to.join(&name)
        };
        if entry.file_type().expect("file_type").is_dir() {
            copy_into(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

fn needs_materialising(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .expect("read_dir")
        .filter_map(Result::ok)
        .any(|e| {
            e.file_name() == DOT_GIT
                || (e.file_type().is_ok_and(|t| t.is_dir()) && needs_materialising(&e.path()))
        })
}

fn prepare(vault: &Path) -> Prepared {
    if !needs_materialising(vault) {
        return Prepared::Committed(vault.to_path_buf());
    }
    let temp = tempfile::tempdir().expect("tempdir");
    copy_into(vault, temp.path());
    Prepared::Materialised(temp)
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn codes_raised(vault: &Path) -> BTreeSet<Code> {
    let prepared = prepare(vault);
    let vault = prepared.path();
    check::root(vault, Env::default())
        .unwrap_or_else(|e| panic!("check {}: {e}", vault.display()))
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn every_rule_has_a_fixture_directory_with_both_polarities() {
    for code in Code::ALL {
        let dir = fixtures().join(code.as_str());
        assert!(dir.is_dir(), "{} has no fixture directory", code.as_str());
        for polarity in ["good", "bad"] {
            let p = dir.join(polarity);
            assert!(p.is_dir(), "{}/{polarity} is missing", code.as_str());
            assert!(
                mnemex::kind::Kind::ALL
                    .iter()
                    .any(|k| p.join(k.folder()).is_dir()),
                "{}/{polarity} holds no governed folder",
                code.as_str()
            );
        }
    }
}

#[test]
fn no_fixture_directory_belongs_to_a_code_that_does_not_exist() {
    let known: BTreeSet<&str> = Code::ALL.iter().map(|c| c.as_str()).collect();
    for entry in std::fs::read_dir(fixtures()).expect("fixtures") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(known.contains(name.as_str()), "{name} is not a rule");
    }
}

#[test]
fn every_bad_fixture_raises_its_rule() {
    for code in Code::ALL {
        let raised = codes_raised(&fixtures().join(code.as_str()).join("bad"));
        assert!(
            raised.contains(&code),
            "{}/bad raises {raised:?}, not it",
            code.as_str()
        );
    }
}

#[test]
fn no_good_fixture_raises_its_rule() {
    for code in Code::ALL {
        let raised = codes_raised(&fixtures().join(code.as_str()).join("good"));
        assert!(
            !raised.contains(&code),
            "{}/good raises it: {raised:?}",
            code.as_str()
        );
    }
}

#[test]
fn every_good_fixture_is_wholly_clean() {
    // A good fixture is not merely free of its own rule: it is a vault the tool
    // has nothing at all to say about.
    for code in Code::ALL {
        let dir = fixtures().join(code.as_str()).join("good");
        let raised = codes_raised(&dir);
        assert!(
            raised.is_empty(),
            "{}/good is not clean: {raised:?}",
            code.as_str()
        );
    }
}
