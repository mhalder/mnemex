//! Acceptance criteria that no other file states outright.

mod support;

use std::path::Path;

use mnemex::authoring::{self, Ctx};
use mnemex::kind::Kind;
use mnemex::spec::Shape;
use support::Vault;

fn ctx<'a>(v: &'a Vault, stamp: &'a str) -> Ctx<'a> {
    Ctx {
        root: v.root(),
        home: None,
        stamp,
    }
}

#[test]
fn every_writing_verb_refuses_frontmatter_it_cannot_rewrite() {
    // A nested value makes a document unrenderable, and no verb may destroy it.
    let nested = "status: active\nnested:\n  a: 1\n";
    let v = Vault::new();
    v.note("projects", "202609061846-pr", nested);
    v.note(
        "plans",
        "202609070817-q",
        "project: \"[[202609061846-pr]]\"\nstatus: open\nnested:\n  a: 1\n",
    );

    // `set` and `rename` rewrite the subject itself.
    for verb in [
        |v: &Vault| authoring::set(ctx(v, "202609081102"), "202609070817-q", "tags", &[], true),
        |v: &Vault| authoring::rename(ctx(v, "202609081102"), "202609070817-q", "New"),
    ] {
        let e = verb(&v).expect_err("refused").to_string();
        assert!(
            e.ends_with("has frontmatter this tool cannot rewrite without losing it"),
            "{e}"
        );
    }
    let q = std::fs::read_to_string(v.root().join("plans/202609070817-q.md")).expect("read");
    assert!(q.contains("nested:"), "the unrenderable note was rewritten");
}

#[test]
fn set_covers_every_settable_field_of_every_kind() {
    for kind in Kind::ALL {
        for field in kind.spec().fields {
            if !authoring::settable_fields(kind).contains(&field.name) {
                continue;
            }
            let v = Vault::new();
            v.note("projects", "202609061846-pr", "status: active\n");
            let id = "202609081102-subject";
            let fm: String = kind
                .spec()
                .fields
                .iter()
                .filter(|f| f.required)
                .map(|f| match f.shape {
                    Shape::WikiLink => "project: \"[[202609061846-pr]]\"\n".to_owned(),
                    Shape::Enum(vs) => format!("{}: {}\n", f.name, vs.first().unwrap_or(&"")),
                    _ => unreachable!("required fields are enum or wiki-link"),
                })
                .collect();
            v.note(kind.folder(), id, &fm);

            let values = match field.shape {
                Shape::Enum(vs) => vec![(*vs.last().unwrap_or(&"")).to_owned()],
                Shape::WikiLink => vec!["202609061846-pr".to_owned()],
                Shape::TextList => vec!["a".to_owned(), "b".to_owned()],
                Shape::UrlList => vec!["https://github.com/owner/name/pull/42".to_owned()],
                Shape::Remote => vec!["git@github.com:owner/mnemex.git".to_owned()],
                Shape::Text => vec!["a value".to_owned()],
            };
            let out = authoring::set(ctx(&v, "202609081102"), id, field.name, &values, false)
                .unwrap_or_else(|e| panic!("{}.{}: {e}", kind.name(), field.name));
            let written = std::fs::read_to_string(&out.path).expect("read");
            assert!(
                written.contains(&format!("{}:", field.name)),
                "{}.{} was not written: {written}",
                kind.name(),
                field.name
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn a_project_rename_that_cannot_finish_leaves_nothing_rewritten() {
    use std::os::unix::fs::PermissionsExt as _;
    let v = Vault::new();
    v.note("projects", "202609061846-pr", "status: active\n");
    v.note(
        "plans",
        "202609070816-a",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );
    v.note(
        "plans",
        "202609070817-b",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );

    let before: Vec<(std::path::PathBuf, String)> = [
        "projects/202609061846-pr.md",
        "plans/202609070816-a.md",
        "plans/202609070817-b.md",
    ]
    .iter()
    .map(|r| {
        (
            v.root().join(r),
            std::fs::read_to_string(v.root().join(r)).expect("read"),
        )
    })
    .collect();

    // The renamed project cannot be written into its folder, after both spoke
    // rewrites have already been made.
    let projects = v.root().join("projects");
    std::fs::set_permissions(&projects, std::fs::Permissions::from_mode(0o555)).expect("chmod");

    let result = authoring::rename(ctx(&v, "202609081102"), "202609061846-pr", "New Name");
    std::fs::set_permissions(&projects, std::fs::Permissions::from_mode(0o755)).expect("restore");

    assert!(result.is_err(), "the rename could not finish");
    assert!(
        v.root().join("projects/202609061846-pr.md").is_file(),
        "nothing was moved"
    );
    assert!(!v.root().join("projects/202609061846-new-name.md").exists());
    for (path, original) in before {
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            original,
            "{} was left rewritten",
            path.display()
        );
    }
    let _ = Path::new("/");
}

#[cfg(unix)]
#[test]
fn a_project_rename_that_cannot_rewrite_a_spoke_leaves_everything() {
    use std::os::unix::fs::PermissionsExt as _;
    let v = Vault::new();
    v.note("projects", "202609061846-pr", "status: active\n");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );

    // The spoke's folder is read-only, so its rewrite fails before the project
    // is touched.
    let plans = v.root().join("plans");
    std::fs::set_permissions(&plans, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    let result = authoring::rename(ctx(&v, "202609081102"), "202609061846-pr", "New");
    std::fs::set_permissions(&plans, std::fs::Permissions::from_mode(0o755)).expect("restore");

    assert!(result.is_err(), "the rewrite could not finish");
    assert!(v.root().join("projects/202609061846-pr.md").is_file());
    assert!(!v.root().join("projects/202609061846-new.md").exists());
    let x = std::fs::read_to_string(v.root().join("plans/202609070816-x.md")).expect("read");
    assert!(
        x.contains("[[202609061846-pr]]"),
        "the spoke was left as it was: {x}"
    );
}
