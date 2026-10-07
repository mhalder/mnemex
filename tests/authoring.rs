//! The authoring verbs, driven through the library.

mod support;

use std::path::Path;

use mnemex::authoring::{self, Ctx, NewArgs, Outcome};
use mnemex::kind::Kind;
use support::Vault;

const STAMP: &str = "202609081102";

fn ctx<'a>(v: &'a Vault, home: Option<&'a Path>) -> Ctx<'a> {
    Ctx {
        root: v.root(),
        home,
        stamp: STAMP,
    }
}

fn scalar(path: &Path, name: &str) -> Option<String> {
    let src = std::fs::read_to_string(path).expect("read");
    mnemex::frontmatter::parse(&src)
        .expect("parse")
        .get(name)
        .and_then(|f| f.value.as_scalar().map(ToOwned::to_owned))
}

fn project(v: &Vault, id: &str) {
    v.note("projects", id, "status: active\n");
}

/// The id `new` mints for `title`.
fn minted(title: &str) -> String {
    format!("{STAMP}-{}", mnemex::id::slugify(title))
}

// --- new -------------------------------------------------------------------

#[test]
fn new_project_writes_status_and_the_scaffold() {
    let v = Vault::new();
    let out =
        authoring::new(ctx(&v, None), Kind::Project, "mnemex", NewArgs::default()).expect("new");
    assert_eq!(
        out.path,
        v.root().join(format!("projects/{}.md", minted("mnemex")))
    );
    let src = std::fs::read_to_string(&out.path).expect("read");
    assert!(src.starts_with("---\nstatus: active\n---\n"), "{src}");
    assert!(src.contains("## Purpose"), "{src}");
    assert!(out.diagnostics.is_empty());
}

#[test]
fn new_spoke_writes_project_link_and_status() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let out = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "A Plan",
        NewArgs {
            project: Some("202609061846-p"),
            ..NewArgs::default()
        },
    )
    .expect("new");
    let src = std::fs::read_to_string(&out.path).expect("read");
    assert!(src.contains("project: \"[[202609061846-p]]\""), "{src}");
    assert!(src.contains("status: open"), "{src}");
}

#[test]
fn new_requires_project_for_spokes_and_refuses_it_for_a_project() {
    let v = Vault::new();
    let e =
        authoring::new(ctx(&v, None), Kind::Memory, "M", NewArgs::default()).expect_err("refused");
    assert_eq!(e.to_string(), "a memory needs `--project <project-id>`");

    let e = authoring::new(
        ctx(&v, None),
        Kind::Project,
        "P",
        NewArgs {
            project: Some("202609061846-p"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "a project has no `project` field, so `--project` does not apply"
    );
}

#[test]
fn new_refuses_an_unknown_or_wrong_kind_project() {
    let v = Vault::new();
    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609079999-gone"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`202609079999-gone` is no note in this vault"
    );

    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-q",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609070816-q"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`202609070816-q` is a plan in `plans/`, not a project"
    );
}

#[test]
fn status_is_accepted_for_plan_and_adr_only() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let out = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609061846-p"),
            status: Some("done"),
            ..NewArgs::default()
        },
    )
    .expect("new");
    assert_eq!(scalar(&out.path, "status").as_deref(), Some("done"));

    let e = authoring::new(
        ctx(&v, None),
        Kind::Memory,
        "M",
        NewArgs {
            project: Some("202609061846-p"),
            status: Some("open"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(e.to_string(), "`--status` does not apply to a memory");

    let e = authoring::new(
        ctx(&v, None),
        Kind::Adr,
        "A",
        NewArgs {
            project: Some("202609061846-p"),
            status: Some("nope"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`nope` is not a `status`; choose one of `proposed`, `accepted`, `deprecated`"
    );
}

#[test]
fn refs_are_normalised_deduplicated_and_refused_where_they_do_not_apply() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let out = authoring::new(
        ctx(&v, None),
        Kind::Memory,
        "M",
        NewArgs {
            project: Some("202609061846-p"),
            refs: &[
                "HTTPS://GitHub.com/o/r/pull/42/".to_owned(),
                "https://github.com/o/r/pull/42#files".to_owned(),
            ],
            ..NewArgs::default()
        },
    )
    .expect("new");
    let src = std::fs::read_to_string(&out.path).expect("read");
    assert!(
        src.contains("refs:\n  - https://github.com/o/r/pull/42\n"),
        "deduplicated to one canonical entry: {src}"
    );

    let e = authoring::new(
        ctx(&v, None),
        Kind::Context,
        "C",
        NewArgs {
            project: Some("202609061846-p"),
            refs: &["https://github.com/o/r/pull/1".to_owned()],
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(e.to_string(), "`--ref` does not apply to a context");

    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609061846-p"),
            refs: &["not a url".to_owned()],
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert!(
        e.to_string()
            .starts_with("`not a url` is not a work-item URL:"),
        "{e}"
    );
}

#[test]
fn path_and_repo_are_project_only() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609061846-p"),
            path: Some("x"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "only a project has a `path`, so `--path` does not apply to a plan"
    );
}

#[test]
fn new_path_derives_repo_from_the_checkout() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:owner/mnemex.git");
    let out = authoring::new(
        ctx(&v, None),
        Kind::Project,
        "P",
        NewArgs {
            path: Some("work"),
            ..NewArgs::default()
        },
    )
    .expect("new");
    assert_eq!(
        scalar(&out.path, "repo").as_deref(),
        Some("github.com/owner/mnemex")
    );
}

// --- set -------------------------------------------------------------------

#[test]
fn set_project_moves_a_spoke() {
    let v = Vault::new();
    project(&v, "202609061846-a");
    project(&v, "202609061847-b");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-a]]\"\nstatus: open\n",
    );
    let out = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "project",
        &["202609061847-b".to_owned()],
        false,
    )
    .expect("set");
    assert_eq!(
        scalar(&out.path, "project").as_deref(),
        Some("[[202609061847-b]]")
    );
}

#[test]
fn set_project_refuses_a_non_project_and_a_missing_note() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-q",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    v.note(
        "plans",
        "202609070817-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let e = authoring::set(
        ctx(&v, None),
        "202609070817-x",
        "project",
        &["202609070816-q".to_owned()],
        false,
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`202609070816-q` is a plan in `plans/`, not a project"
    );
}

#[test]
fn set_refs_normalises_and_a_blank_ref_is_refused() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let out = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "refs",
        &["https://github.com/o/r/pull/1".to_owned()],
        false,
    )
    .expect("set");
    assert!(
        scalar(&out.path, "refs").is_none(),
        "a list is not a scalar"
    );
    let src = std::fs::read_to_string(&out.path).expect("read");
    assert!(
        src.contains("refs:\n  - https://github.com/o/r/pull/1\n"),
        "{src}"
    );

    let e = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "refs",
        &["   ".to_owned()],
        false,
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`refs` cannot hold a blank value; remove the empty one"
    );
}

#[test]
fn set_status_refuses_an_off_enum_value() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let e = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "status",
        &["maybe".to_owned()],
        false,
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`maybe` is not a `status`; choose one of `open`, `done`"
    );
}

#[test]
fn set_path_stores_home_relative() {
    let v = Vault::new();
    let home = v.dir("home");
    let work = v.dir("home/src/work");
    v.note("projects", "202609061846-p", "status: active\n");
    let out = authoring::set(
        ctx(&v, Some(&home)),
        "202609061846-p",
        "path",
        &[work.display().to_string()],
        false,
    )
    .expect("set");
    assert_eq!(scalar(&out.path, "path").as_deref(), Some("~/src/work"));
}

#[test]
fn set_repo_canonicalises() {
    let v = Vault::new();
    v.note("projects", "202609061846-p", "status: active\n");
    let out = authoring::set(
        ctx(&v, None),
        "202609061846-p",
        "repo",
        &["git@github.com:owner/mnemex.git".to_owned()],
        false,
    )
    .expect("set");
    assert_eq!(
        scalar(&out.path, "repo").as_deref(),
        Some("github.com/owner/mnemex")
    );
}

#[test]
fn set_refuses_clearing_a_required_field_and_setting_id() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let e =
        authoring::set(ctx(&v, None), "202609070816-x", "status", &[], true).expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`status` is required, so it cannot be cleared"
    );

    let e = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "id",
        &["z".to_owned()],
        false,
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`id` is not a field: a note's id is its filename — use `mnemex rename`"
    );
}

// --- rename -----------------------------------------------------------------

#[test]
fn rename_keeps_the_timestamp_and_rewrites_spokes() {
    let v = Vault::new();
    v.note("projects", "202609061846-pr", "status: active\n");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );
    let out = authoring::rename(ctx(&v, None), "202609061846-pr", "New Name").expect("rename");
    assert!(
        out.path.ends_with("202609061846-new-name.md"),
        "{}",
        out.path.display()
    );
    assert!(
        out.notes.contains(&"1 link updated".to_owned()),
        "{:?}",
        out.notes
    );
    assert!(!v.root().join("projects/202609061846-pr.md").exists());
    let x = std::fs::read_to_string(v.root().join("plans/202609070816-x.md")).expect("read");
    assert!(x.contains("project: \"[[202609061846-new-name]]\""), "{x}");
}

#[test]
fn rename_preview_reports_without_writing() {
    let v = Vault::new();
    v.note("projects", "202609061846-pr", "status: active\n");
    let out = authoring::rename_preview(ctx(&v, None), "202609061846-pr", "New").expect("preview");
    assert_eq!(
        out.headline,
        "would rename 202609061846-pr to 202609061846-new"
    );
    assert!(v.root().join("projects/202609061846-pr.md").is_file());
}

#[test]
fn rename_refuses_a_collision() {
    let v = Vault::new();
    v.note("projects", "202609061846-a", "status: active\n");
    v.note(
        "plans",
        "202609061846-b",
        "project: \"[[202609061846-a]]\"\nstatus: open\n",
    );
    let e = authoring::rename(ctx(&v, None), "202609061846-a", "b").expect_err("collision");
    assert!(e.to_string().contains("already names a note"), "{e}");
}

// --- delete -----------------------------------------------------------------

#[test]
fn delete_removes_a_spoke_outright() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let out = authoring::delete(ctx(&v, None), "202609070816-x").expect("delete");
    assert_eq!(out.headline, "deleted 202609070816-x");
    assert!(!v.root().join("plans/202609070816-x.md").exists());
}

#[test]
fn delete_reports_a_body_that_still_links_the_removed_note() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    // A body elsewhere links the note about to be removed.
    v.write(
        "memories",
        "202609081100-m",
        "---\nproject: \"[[202609061846-p]]\"\n---\n\n# M\n\nsee [[202609070816-x]]\n",
    );
    let out = authoring::delete(ctx(&v, None), "202609070816-x").expect("delete");
    assert!(
        out.notes
            .iter()
            .any(|n| n.contains("still mentions `202609070816-x`")),
        "{:?}",
        out.notes
    );
}

#[test]
fn delete_refuses_a_project_with_spokes() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: accepted\n",
    );
    let e = authoring::delete(ctx(&v, None), "202609061846-p").expect_err("refused");
    let text = e.to_string();
    assert!(text.contains("`202609070816-x`"), "{text}");
    assert!(text.contains("`202609061400-a`"), "{text}");
    assert!(v.root().join("projects/202609061846-p.md").is_file());
}

// --- adopt ------------------------------------------------------------------

#[test]
fn adopt_keeps_the_body_and_moves_a_stray_file() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let stray = v.root().join("plans/draft.md");
    std::fs::write(&stray, "# My Plan\n\nsteps\n").expect("write");
    let out = authoring::adopt(
        ctx(&v, None),
        Kind::Plan,
        &stray,
        NewArgs {
            project: Some("202609061846-p"),
            ..NewArgs::default()
        },
    )
    .expect("adopt");
    assert!(!stray.exists(), "a stray is moved");
    let src = std::fs::read_to_string(&out.path).expect("read");
    assert!(src.contains("project: \"[[202609061846-p]]\""), "{src}");
    assert!(src.contains("status: open"), "{src}");
    assert!(src.ends_with("# My Plan\n\nsteps\n"), "{src}");
}

#[test]
fn adopt_refuses_the_same_flags_new_refuses() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let file = v.root().join("x.md");
    std::fs::write(&file, "# X\n").expect("write");
    let e = authoring::adopt(
        ctx(&v, None),
        Kind::Memory,
        &file,
        NewArgs {
            project: Some("202609061846-p"),
            status: Some("open"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(e.to_string(), "`--status` does not apply to a memory");
}

#[test]
fn outcome_holds_the_notes_diagnostics() {
    let v = Vault::new();
    let out = authoring::new(ctx(&v, None), Kind::Project, "P", NewArgs::default()).expect("new");
    assert!(out.diagnostics.is_empty());
    let _: Outcome = out;
}

#[test]
fn set_refuses_wrong_arity_unknown_fields_and_bad_values() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );

    // More than one value for a scalar.
    let e = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "status",
        &["open".to_owned(), "done".to_owned()],
        false,
    )
    .expect_err("refused");
    assert_eq!(e.to_string(), "`status` takes one value, not 2");

    // An unknown field.
    let e = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "nope",
        &["x".to_owned()],
        false,
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "a `plan` has no `nope`; its fields are `project`, `status`, `tags`, `refs`"
    );

    // A list with no values.
    let e =
        authoring::set(ctx(&v, None), "202609070816-x", "tags", &[], false).expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`tags` needs at least one value; `mnemex set 202609070816-x tags --clear` removes it"
    );

    // A wiki-link carrying a `|`.
    let e = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "project",
        &["a|b".to_owned()],
        false,
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`a|b` is not a wiki-link: a link is `[[id]]`, with no display text"
    );

    // A blank tag.
    let e = authoring::set(
        ctx(&v, None),
        "202609070816-x",
        "tags",
        &["  ".to_owned()],
        false,
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`tags` cannot hold a blank value; remove the empty one"
    );
}

#[test]
fn new_refuses_a_control_character_title_and_blank_tag_or_ref() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "a\tb",
        NewArgs {
            project: Some("202609061846-p"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert!(e.to_string().contains("holds a control character"), "{e}");

    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609061846-p"),
            tags: &["  ".to_owned()],
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`tags` cannot hold a blank value; remove the empty one"
    );

    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609061846-p"),
            refs: &["  ".to_owned()],
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`refs` cannot hold a blank value; remove the empty one"
    );
}

#[test]
fn new_writes_tags() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let out = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609061846-p"),
            tags: &["a".to_owned(), "b".to_owned()],
            ..NewArgs::default()
        },
    )
    .expect("new");
    let src = std::fs::read_to_string(&out.path).expect("read");
    assert!(src.contains("tags:\n  - a\n  - b\n"), "{src}");
}

#[test]
fn adopt_with_no_heading_uses_the_file_stem_and_leaves_foreign_files() {
    let v = Vault::new();
    project(&v, "202609061846-p");

    // A file with no `# ` heading: its title is the stem.
    let plain = v.root().join("plain.md");
    std::fs::write(&plain, "just a body\n").expect("write");
    let out = authoring::adopt(
        ctx(&v, None),
        Kind::Plan,
        &plain,
        NewArgs {
            project: Some("202609061846-p"),
            ..NewArgs::default()
        },
    )
    .expect("adopt");
    let src = std::fs::read_to_string(&out.path).expect("read");
    assert!(src.contains("# plain"), "{src}");
    assert!(
        out.notes
            .iter()
            .any(|n| n.contains("the body had no `# ` heading")),
        "{:?}",
        out.notes
    );

    // A foreign file (outside the vault) is left where it was.
    let foreign = std::env::temp_dir().join(format!("memex-adopt-{}.md", std::process::id()));
    std::fs::write(&foreign, "# Foreign\n\nbody\n").expect("write");
    let out = authoring::adopt(
        ctx(&v, None),
        Kind::Memory,
        &foreign,
        NewArgs {
            project: Some("202609061846-p"),
            ..NewArgs::default()
        },
    )
    .expect("adopt");
    assert!(foreign.is_file(), "a foreign file is left in place");
    assert!(
        out.notes
            .iter()
            .any(|n| n.contains("was left where it was")),
        "{:?}",
        out.notes
    );
    let _ = std::fs::remove_file(&foreign);
}

#[test]
fn adopt_and_rename_refuse_a_collision() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    // A note already holds the id adopt would mint.
    v.note(
        "plans",
        "202609081102-draft",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let file = v.root().join("draft.md");
    std::fs::write(&file, "# Draft\n").expect("write");
    let e = authoring::adopt(
        ctx(&v, None),
        Kind::Plan,
        &file,
        NewArgs {
            project: Some("202609061846-p"),
            ..NewArgs::default()
        },
    )
    .expect_err("collision");
    assert!(e.to_string().contains("already exists"), "{e}");
    let _ = std::fs::remove_file(&file);
}

#[test]
fn rename_into_an_existing_path_refuses() {
    let v = Vault::new();
    v.note("projects", "202609061846-a", "status: active\n");
    v.note("projects", "202609061846-b", "status: active\n");
    let e = authoring::rename(ctx(&v, None), "202609061846-a", "b").expect_err("collision");
    assert!(e.to_string().contains("already exists"), "{e}");

    // The preview refuses the same thing, and reports the links it would update.
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-a]]\"\nstatus: open\n",
    );
    let out = authoring::rename_preview(ctx(&v, None), "202609061846-a", "C").expect("preview");
    assert!(
        out.notes
            .iter()
            .any(|n| n.contains("1 link would be updated")),
        "{:?}",
        out.notes
    );
}

#[test]
fn new_refuses_when_the_minted_path_exists() {
    let v = Vault::new();
    v.note("projects", "202609081102-p", "status: active\n");
    let e =
        authoring::new(ctx(&v, None), Kind::Project, "P", NewArgs::default()).expect_err("exists");
    assert!(e.to_string().contains("already exists"), "{e}");
}

#[test]
fn adopt_normalises_refs_and_set_refs_needs_a_value() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let file = v.root().join("m.md");
    std::fs::write(&file, "# M\n").expect("write");
    let out = authoring::adopt(
        ctx(&v, None),
        Kind::Memory,
        &file,
        NewArgs {
            project: Some("202609061846-p"),
            refs: &["HTTPS://GitHub.com/o/r/pull/1/".to_owned()],
            ..NewArgs::default()
        },
    )
    .expect("adopt");
    let src = std::fs::read_to_string(&out.path).expect("read");
    assert!(src.contains("https://github.com/o/r/pull/1"), "{src}");
    let _ = std::fs::remove_file(&file);

    // `set refs` with no values is a refusal, not an empty list.
    let e = authoring::set(
        ctx(&v, None),
        &out.path.file_stem().unwrap().to_string_lossy(),
        "refs",
        &[],
        false,
    )
    .expect_err("refused");
    assert!(e.to_string().contains("needs at least one value"), "{e}");
}

#[test]
fn rename_preview_refuses_an_existing_path() {
    let v = Vault::new();
    v.note("projects", "202609061846-a", "status: active\n");
    v.note("projects", "202609061846-b", "status: active\n");
    let e = authoring::rename_preview(ctx(&v, None), "202609061846-a", "b").expect_err("exists");
    assert!(e.to_string().contains("already exists"), "{e}");
}

#[test]
fn a_rename_skips_an_unrenderable_spoke_and_reports_body_mentions() {
    let v = Vault::new();
    v.note("projects", "202609061846-pr", "status: active\n");
    // A parseable-but-unrenderable spoke still names the project.
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-pr]]\"\nstatus: open\nnested:\n  a: 1\n",
    );
    // A body elsewhere that mentions the old id.
    v.write(
        "memories",
        "202609081100-m",
        "---\nproject: \"[[202609061846-pr]]\"\n---\n\n# M\n\nsee [[202609061846-pr]]\n",
    );
    let out = authoring::rename(ctx(&v, None), "202609061846-pr", "New").expect("rename");
    assert!(
        out.notes
            .iter()
            .any(|n| n.contains("cannot rewrite without losing it")),
        "{:?}",
        out.notes
    );
    assert!(
        out.notes
            .iter()
            .any(|n| n.contains("still mentions `202609061846-pr`")),
        "{:?}",
        out.notes
    );
}
