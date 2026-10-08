//! `repo`: the verbs that write it, and the checkout it comes from.
//!
//! `path` says where the work happens on this machine; `repo` says which
//! repository the work *is*. The tool derives the second from the first
//! wherever it can, because a field a human has to type twice is a field that
//! drifts — and reports every derivation, because a silent write is worse than
//! a missing one.

mod support;

use std::path::Path;

use mnemex::authoring::{self, Ctx, NewArgs, Outcome};
use mnemex::check::{self, Env};
use mnemex::diagnostic::{Code, Severity};
use mnemex::kind::Kind;
use support::Vault;

const STAMP: &str = "202609081102";
const URL: &str = "git@github.com:owner/mnemex.git";
const CANONICAL: &str = "github.com/owner/mnemex";

fn ctx<'a>(v: &'a Vault, home: Option<&'a Path>) -> Ctx<'a> {
    Ctx {
        root: v.root(),
        home,
        stamp: STAMP,
    }
}

fn field(out: &Outcome, name: &str) -> Option<String> {
    let src = std::fs::read_to_string(&out.path).expect("read");
    mnemex::frontmatter::parse(&src)
        .expect("parse")
        .get(name)
        .and_then(|f| f.value.as_scalar().map(ToOwned::to_owned))
}

fn new_project(v: &Vault, args: NewArgs<'_>) -> Outcome {
    authoring::new(ctx(v, None), Kind::Project, "The Project", args).expect("new")
}

/// The verbs take any spelling and store the one form, exactly as a `project`
/// link takes a bare id and stores `[[id]]`.
#[test]
fn new_canonicalises_the_repo_it_is_given() {
    let v = Vault::new();
    let out = new_project(
        &v,
        NewArgs {
            repo: Some("https://GitHub.com/owner/mnemex.git"),
            ..NewArgs::default()
        },
    );
    assert_eq!(field(&out, "repo").as_deref(), Some(CANONICAL));
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

/// Only a project has a `repo`, and a remote with no host is refused before
/// anything is written.
#[test]
fn repo_is_refused_where_it_does_not_apply_or_cannot_be_canonicalised() {
    let v = Vault::new();
    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "A Plan",
        NewArgs {
            project: Some("202609061846-p"),
            repo: Some(URL),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "only a project has a `repo`, so `--repo` does not apply to a plan"
    );

    let e = authoring::new(
        ctx(&v, None),
        Kind::Project,
        "The Project",
        NewArgs {
            repo: Some("/srv/git/name.git"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`/srv/git/name.git` is not a git remote: it needs a host, an owner and a name, \
         as in `git@github.com:owner/name.git` — a checkout on this machine is what `path` names"
    );
    assert!(
        !v.root()
            .join("projects")
            .join(format!("{STAMP}-the-project.md"))
            .exists(),
        "the note was written before the remote was judged"
    );
}

/// `set` canonicalises too, and refuses the same values `new` refuses.
#[test]
fn set_canonicalises_and_clears() {
    let v = Vault::new();
    let created = new_project(&v, NewArgs::default());
    let id = format!("{STAMP}-the-project");

    let out = authoring::set(ctx(&v, None), &id, "repo", &[URL.to_owned()], false).expect("set");
    assert_eq!(field(&out, "repo").as_deref(), Some(CANONICAL));

    let e = authoring::set(
        ctx(&v, None),
        &id,
        "repo",
        &["not a remote".to_owned()],
        false,
    )
    .expect_err("refused");
    assert!(
        e.to_string()
            .starts_with("`not a remote` is not a git remote")
    );

    let out = authoring::set(ctx(&v, None), &id, "repo", &[], true).expect("clear");
    assert_eq!(field(&out, "repo"), None);
    let _ = created;
}

/// The whole point of the field: naming the directory fills it in, and says so.
#[test]
fn a_path_that_is_a_checkout_fills_in_the_repo() {
    let v = Vault::new();
    v.checkout("work", URL);
    let out = new_project(
        &v,
        NewArgs {
            path: Some("work"),
            ..NewArgs::default()
        },
    );
    assert_eq!(field(&out, "repo").as_deref(), Some(CANONICAL));
    assert!(
        out.notes.iter().any(|n| n.contains(CANONICAL)),
        "the derivation was not reported: {:?}",
        out.notes
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

/// A given `repo` is never overwritten by a derived one: the argument is the
/// claim, and a disagreement is `MX404`'s to report, not the verb's to resolve.
#[test]
fn a_given_repo_survives_a_path_that_disagrees() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:someone/else.git");
    let out = new_project(
        &v,
        NewArgs {
            path: Some("work"),
            repo: Some(URL),
            ..NewArgs::default()
        },
    );
    assert_eq!(field(&out, "repo").as_deref(), Some(CANONICAL));
    let codes: Vec<Code> = out.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, [Code::Mx404], "{:?}", out.diagnostics);
}

/// `set path` fills the field in the same way, and leaves an existing one be.
#[test]
fn setting_a_path_fills_in_an_absent_repo_and_leaves_a_present_one() {
    let v = Vault::new();
    v.checkout("work", URL);
    let created = new_project(&v, NewArgs::default());
    let id = format!("{STAMP}-the-project");
    assert_eq!(field(&created, "repo"), None);

    let out = authoring::set(ctx(&v, None), &id, "path", &["work".to_owned()], false).expect("set");
    assert_eq!(field(&out, "repo").as_deref(), Some(CANONICAL));

    v.checkout("other", "git@github.com:someone/else.git");
    let out =
        authoring::set(ctx(&v, None), &id, "path", &["other".to_owned()], false).expect("set");
    assert_eq!(
        field(&out, "repo").as_deref(),
        Some(CANONICAL),
        "a derived value overwrote a stored one"
    );
}

/// A project whose path is a checkout but has no repository identity is drift:
/// the machine-local half exists, while the portable half is missing.
#[test]
fn a_checkout_path_without_a_repo_is_a_warning() {
    let v = Vault::new();
    v.checkout("work", URL);
    let path = v.note("projects", "202609061846-p", "status: active\npath: work\n");

    let ds = check::path(&path, v.root(), Env::default()).expect("check");
    let codes: Vec<Code> = ds.iter().map(|d| d.code).collect();
    assert_eq!(codes, [Code::Mx405], "{ds:?}");
    assert_eq!(ds[0].severity, Severity::Warning);
    assert!(ds[0].message.contains(CANONICAL), "{:?}", ds[0]);
}

#[test]
fn a_checkout_without_a_remote_still_requires_a_repo() {
    let v = Vault::new();
    let work = v.dir("work");
    std::fs::create_dir(work.join(".git")).expect("git dir");
    let path = v.note("projects", "202609061846-p", "status: active\npath: work\n");

    let ds = check::path(&path, v.root(), Env::default()).expect("check");
    let codes: Vec<Code> = ds.iter().map(|d| d.code).collect();
    assert_eq!(codes, [Code::Mx405], "{ds:?}");
    assert!(
        ds[0].message.contains("no remote this tool can store"),
        "the repair must not name a remote that does not exist: {:?}",
        ds[0]
    );
}

/// With one remote that is not `origin`, the repair may name it: the checkout
/// has no other candidate to be confused with.
#[test]
fn one_remote_no_origin_is_still_suggested() {
    let v = Vault::new();
    v.checkout_with("work", &[("private", "git@github.com:owner/mnemex.git")]);
    let path = v.note("projects", "202609061846-p", "status: active\npath: work\n");

    let ds = check::path(&path, v.root(), Env::default()).expect("check");
    let codes: Vec<Code> = ds.iter().map(|d| d.code).collect();
    assert_eq!(codes, [Code::Mx405], "{ds:?}");
    assert!(ds[0].message.contains(CANONICAL), "{:?}", ds[0]);
}

/// With several remotes and no `origin`, the repair names them and picks none:
/// which one a project is is the writer's call.
#[test]
fn several_remotes_without_an_origin_are_named_but_not_chosen() {
    let v = Vault::new();
    v.checkout_with(
        "work",
        &[
            ("base", "ssh://git@ssh.example.com/owner/notes-base.git"),
            (
                "private",
                "ssh://git@ssh.example.com/owner/notes-private.git",
            ),
        ],
    );
    let path = v.note("projects", "202609061846-p", "status: active\npath: work\n");

    let ds = check::path(&path, v.root(), Env::default()).expect("check");
    let codes: Vec<Code> = ds.iter().map(|d| d.code).collect();
    assert_eq!(codes, [Code::Mx405], "{ds:?}");
    assert!(
        ds[0].message.contains("notes-base") && ds[0].message.contains("notes-private"),
        "the repair must name every remote: {:?}",
        ds[0]
    );
    assert!(
        ds[0].message.contains("none is `origin`"),
        "the repair must not choose among them: {:?}",
        ds[0]
    );
}

#[test]
fn a_blank_repo_on_a_checkout_path_is_missing() {
    let v = Vault::new();
    v.checkout("work", URL);
    let path = v.note(
        "projects",
        "202609061846-p",
        "status: active\npath: work\nrepo:\n",
    );

    let ds = check::path(&path, v.root(), Env::default()).expect("check");
    let codes: Vec<Code> = ds.iter().map(|d| d.code).collect();
    assert_eq!(codes, [Code::Mx405], "{ds:?}");
}

/// A directory that is no checkout is silence: no field, no note, no finding.
#[test]
fn a_path_that_is_no_checkout_derives_nothing() {
    let v = Vault::new();
    v.dir("work");
    let out = new_project(
        &v,
        NewArgs {
            path: Some("work"),
            ..NewArgs::default()
        },
    );
    assert_eq!(field(&out, "repo"), None);
    assert!(out.notes.is_empty(), "{:?}", out.notes);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

/// `MX404` compares identities, not spellings: the same repository cloned over
/// https and over ssh is one repository.
#[test]
fn the_drift_rule_compares_identities_not_spellings() {
    let v = Vault::new();
    v.checkout("work", "https://github.com/owner/mnemex.git");
    let out = new_project(
        &v,
        NewArgs {
            path: Some("work"),
            repo: Some("git@github.com:owner/mnemex.git"),
            ..NewArgs::default()
        },
    );
    let ds = check::path(&out.path, v.root(), Env::default()).expect("check");
    assert!(ds.is_empty(), "{ds:?}");
}

/// A checkout may know its work by a remote that is not `origin` — a fork kept
/// beside an upstream, a vault whose remotes name the layers it serves — so a
/// claim is matched against every remote, not just the first-named one.
#[test]
fn a_repo_that_names_a_remote_other_than_origin_is_no_drift() {
    let v = Vault::new();
    v.checkout_with(
        "work",
        &[
            ("origin", "https://github.com/zmkfirmware/zmk.git"),
            ("private", "git@git.sr.ht:~owner/zmk-config"),
        ],
    );
    let out = new_project(
        &v,
        NewArgs {
            path: Some("work"),
            repo: Some("git@git.sr.ht:~owner/zmk-config"),
            ..NewArgs::default()
        },
    );
    let ds = check::path(&out.path, v.root(), Env::default()).expect("check");
    assert!(ds.is_empty(), "{ds:?}");
}

/// A checkout with no `origin` is checked by its other remotes rather than
/// skipped, so a claim that is none of them is drift, named after all of them.
#[test]
fn a_claim_that_is_no_remote_of_the_checkout_is_drift() {
    let v = Vault::new();
    v.checkout_with(
        "work",
        &[
            ("base", "ssh://git@ssh.example.com/owner/notes-base.git"),
            (
                "private",
                "ssh://git@ssh.example.com/owner/notes-private.git",
            ),
        ],
    );
    let out = new_project(
        &v,
        NewArgs {
            path: Some("work"),
            repo: Some("ssh://git@ssh.example.com/owner/notes.git"),
            ..NewArgs::default()
        },
    );
    let codes: Vec<Code> = out.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, [Code::Mx404], "{:?}", out.diagnostics);
    assert!(
        out.diagnostics[0].message.contains("notes-base")
            && out.diagnostics[0].message.contains("notes-private"),
        "the warning must name the remotes it found: {:?}",
        out.diagnostics[0]
    );

    let path = v.note(
        "projects",
        "202609061846-q",
        "status: active\npath: work\nrepo: ssh.example.com/owner/notes-base\n",
    );
    let ds = check::path(&path, v.root(), Env::default()).expect("check");
    assert!(ds.is_empty(), "{ds:?}");
}
