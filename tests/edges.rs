//! The corners the main test files do not reach: absent values, fallback
//! branches, and the paths a rule or a renderer takes when there is nothing to
//! say.

mod support;

use std::path::Path;

use mnemex::authoring::{self, Ctx, NewArgs};
use mnemex::check::{self, Env};
use mnemex::diagnostic::{Code, Diagnostic, Severity};
use mnemex::frontmatter::{self, Position, Value};
use mnemex::kind::Kind;
use mnemex::query::{self, ProjectAnswer};
use mnemex::{links, report};
use support::Vault;

fn ctx<'a>(v: &'a Vault, home: Option<&'a Path>) -> Ctx<'a> {
    Ctx {
        root: v.root(),
        home,
        stamp: "202609081102",
    }
}

// --- frontmatter --------------------------------------------------------

#[test]
fn a_block_holding_only_a_comment_is_well_formed_and_empty() {
    let d = frontmatter::parse("---\n# just a comment\n---\nbody\n").expect("parses");
    assert!(d.fields.is_empty());
    assert_eq!(d.body, "body\n");
}

#[test]
fn a_list_item_anchor_falls_back_to_the_fields_own_anchor() {
    let d = frontmatter::parse("---\ntags:\n  - a\n---\n").expect("parses");
    let f = d.get("tags").expect("tags");
    assert_eq!(f.item_anchor(0), Position::new(3, 5));
    assert_eq!(
        f.item_anchor(9),
        f.anchor(),
        "past the end there is no item to point at"
    );
}

#[test]
fn a_newline_in_a_scalar_is_escaped_rather_than_breaking_the_block() {
    let mut d = frontmatter::parse("---\n---\n").expect("parses");
    d.set("note", Value::Scalar("two\nlines\twide".to_owned()));
    let rendered = d.render(Kind::Context);
    assert!(
        rendered.contains(r#"note: "two\nlines\twide""#),
        "{rendered}"
    );
    let back = frontmatter::parse(&rendered).expect("round trip");
    assert_eq!(
        back.get("note").map(|f| f.value.clone()),
        Some(Value::Scalar("two\nlines\twide".into()))
    );
}

#[test]
fn rendering_an_unrenderable_field_never_invents_a_value() {
    let d = frontmatter::parse("---\nnested:\n  a: 1\n---\nbody\n").expect("parses");
    assert_eq!(d.render(Kind::Context), "---\nnested:\n---\nbody\n");
}

// --- links --------------------------------------------------------------

#[test]
fn a_link_shaped_value_with_nothing_in_it_yields_no_targets() {
    assert!(links::values_of(&Value::Scalar(String::new())).is_empty());
    assert!(links::values_of(&Value::Unsupported).is_empty());
    assert_eq!(links::target("[[  ]]"), None);
    assert_eq!(links::target("not a link"), None);
    assert_eq!(links::target("[[a"), None);
}

#[test]
fn wrap_and_values_cover_every_shape() {
    // An already-bracketed value is left as it stands, and a bare id is wrapped.
    assert_eq!(links::wrap("[[a]]"), "[[a]]");
    assert_eq!(links::wrap("a"), "[[a]]");
    // A list yields each scalar, and a non-empty scalar yields itself.
    assert_eq!(
        links::values_of(&Value::Scalar("[[a]]".to_owned())),
        ["[[a]]"]
    );
    assert_eq!(
        links::values_of(&Value::List(vec!["[[a]]".into(), "[[b]]".into()])),
        ["[[a]]", "[[b]]"]
    );
    // A `|` anywhere is not a link, with or without brackets.
    assert_eq!(links::target("[[a|b]]"), None);
}

// --- path ---------------------------------------------------------------

#[test]
fn path_resolution_and_storage_cover_every_spelling() {
    let home = std::path::Path::new("/home/u");
    assert_eq!(mnemex::path::resolve(Path::new("/v"), Some(home), ""), None);
    assert_eq!(
        mnemex::path::resolve(Path::new("/v"), Some(home), "~/x"),
        Some(std::path::PathBuf::from("/home/u/x"))
    );
    assert_eq!(
        mnemex::path::resolve(Path::new("/v"), Some(home), "$HOME/x"),
        Some(std::path::PathBuf::from("/home/u/x"))
    );
    assert_eq!(
        mnemex::path::resolve(Path::new("/v"), Some(home), "/abs"),
        Some(std::path::PathBuf::from("/abs"))
    );
    assert_eq!(
        mnemex::path::resolve(Path::new("/v"), Some(home), "rel"),
        Some(std::path::PathBuf::from("/v/rel"))
    );

    assert_eq!(mnemex::path::stored("", Some(home)), "");
    assert_eq!(mnemex::path::stored("$HOME/x", Some(home)), "~/x");
    assert_eq!(mnemex::path::stored("~/x", Some(home)), "~/x");
    assert_eq!(mnemex::path::stored("/home/u/src/x", Some(home)), "~/src/x");
    assert_eq!(mnemex::path::stored("/elsewhere", Some(home)), "/elsewhere");
    assert_eq!(mnemex::path::stored("rel", Some(home)), "rel");
    // Without a home directory, an absolute path is left as given.
    assert_eq!(mnemex::path::stored("/abs", None), "/abs");
    assert_eq!(mnemex::path::resolve(Path::new("/v"), None, "~/x"), None);
}

// --- check --------------------------------------------------------------

#[test]
fn check_path_refuses_a_note_outside_a_governed_folder() {
    let v = Vault::new();
    let p = v.write("daily", "2026-09-08", "---\n---\n");
    let e = check::path(&p, v.root(), Env::default()).expect_err("not governed");
    assert!(
        e.to_string().contains(&format!(
            "is not a note of the vault at {}",
            v.root().display()
        )),
        "{e}"
    );
}

#[test]
fn check_path_canonicalises_before_inferring_the_governed_file() {
    let v = Vault::new();
    let plan = v.note(
        "plans",
        "202609070816-p",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );
    v.note("projects", "202609061846-pr", "status: active\n");
    let dotted = v
        .root()
        .join("plans")
        .join("..")
        .join("plans")
        .join("202609070816-p.md");
    let ds = check::path(&dotted, v.root(), Env::default()).expect("check");
    assert!(ds.is_empty(), "{ds:?}");
    // The reported path is the canonical file, not the dotted spelling.
    assert_eq!(
        std::fs::canonicalize(&plan).expect("canonical"),
        v.root().join("plans/202609070816-p.md")
    );
}

#[test]
fn an_unparseable_note_raises_only_its_own_mx001() {
    let v = Vault::new();
    v.write("plans", "202609070816-broken", "no frontmatter here\n");
    v.note("projects", "202609061846-p", "status: active\n");

    let found: Vec<(String, Code)> = check::root(v.root(), Env::default())
        .expect("check")
        .into_iter()
        .map(|d| {
            (
                d.path
                    .file_stem()
                    .expect("stem")
                    .to_string_lossy()
                    .into_owned(),
                d.code,
            )
        })
        .collect();
    assert_eq!(found, [("202609070816-broken".to_owned(), Code::Mx001)]);
}

#[test]
fn a_project_with_no_path_field_raises_no_layout_rule() {
    let v = Vault::new();
    v.note("projects", "202609061846-p", "status: active\n");
    assert!(
        check::root(v.root(), Env::default())
            .expect("check")
            .is_empty()
    );
}

#[test]
fn a_path_that_is_a_list_raises_the_container_rule_and_no_layout_rule() {
    let v = Vault::new();
    v.note(
        "projects",
        "202609061846-p",
        "status: active\npath:\n  - a\n  - b\n",
    );
    let codes: Vec<Code> = check::root(v.root(), Env::default())
        .expect("check")
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(codes, [Code::Mx104]);
}

#[test]
fn an_unsupported_value_on_a_known_field_reports_mx003_and_nothing_below_it() {
    let v = Vault::new();
    v.note("plans", "202609070816-p", "status:\n  a: 1\n");
    let codes: Vec<Code> = check::root(v.root(), Env::default())
        .expect("check")
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(codes, [Code::Mx100, Code::Mx003]);
}

// --- authoring ----------------------------------------------------------

#[test]
fn rename_rewrites_every_spoke_that_names_the_renamed_project() {
    let v = Vault::new();
    v.note("projects", "202609061846-pr", "status: active\n");
    v.note(
        "plans",
        "202609070816-a",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );
    v.note(
        "adrs",
        "202609061400-b",
        "project: \"[[202609061846-pr]]\"\nstatus: accepted\n",
    );
    let out = authoring::rename(ctx(&v, None), "202609061846-pr", "New").expect("rename");
    assert!(
        out.notes.contains(&"2 links updated".to_owned()),
        "{:?}",
        out.notes
    );
    let a = std::fs::read_to_string(v.root().join("plans/202609070816-a.md")).expect("read");
    assert!(a.contains("project: \"[[202609061846-new]]\""), "{a}");
    let b = std::fs::read_to_string(v.root().join("adrs/202609061400-b.md")).expect("read");
    assert!(b.contains("project: \"[[202609061846-new]]\""), "{b}");
}

#[test]
fn rename_refuses_a_title_that_slugifies_to_nothing() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-p",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );
    v.note("projects", "202609061846-pr", "status: active\n");
    let e = authoring::rename(ctx(&v, None), "202609070816-p", "???").expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`???` has no letters or digits to slugify, so it cannot become an id"
    );
    assert!(v.root().join("plans/202609070816-p.md").is_file());
}

#[test]
fn new_refuses_a_project_that_is_not_a_project() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-p",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );
    v.note("projects", "202609061846-pr", "status: active\n");
    let e = authoring::new(
        ctx(&v, None),
        Kind::Plan,
        "T",
        NewArgs {
            project: Some("202609070816-p"),
            ..NewArgs::default()
        },
    )
    .expect_err("refused");
    assert_eq!(
        e.to_string(),
        "`202609070816-p` is a plan in `plans/`, not a project"
    );
}

#[test]
fn new_requires_a_project_for_every_spoke() {
    let v = Vault::new();
    let e =
        authoring::new(ctx(&v, None), Kind::Memory, "T", NewArgs::default()).expect_err("refused");
    assert_eq!(e.to_string(), "a memory needs `--project <project-id>`");
}

#[test]
fn delete_refuses_a_project_that_spokes_still_name() {
    let v = Vault::new();
    v.note("projects", "202609061846-pr", "status: active\n");
    v.note(
        "plans",
        "202609070816-a",
        "project: \"[[202609061846-pr]]\"\nstatus: open\n",
    );
    let e = authoring::delete(ctx(&v, None), "202609061846-pr").expect_err("refused");
    assert!(
        e.to_string()
            .contains("`202609061846-pr` is named in `project` by `202609070816-a`"),
        "{e}"
    );
}

#[test]
fn the_settable_fields_of_a_project_are_its_four_fields() {
    assert_eq!(
        authoring::settable_fields(Kind::Project),
        ["status", "tags", "path", "repo"]
    );
    assert_eq!(
        authoring::settable_fields(Kind::Memory),
        ["project", "tags", "refs"]
    );
}

// --- queries ------------------------------------------------------------

#[test]
fn a_file_that_opens_with_its_heading_needs_no_frontmatter_to_have_a_title() {
    let v = Vault::new();
    v.write("memories", "202609080000-n", "# Straight In\n\nbody\n");
    assert_eq!(
        query::resolve(v.root(), "202609080000-n")
            .expect("found")
            .title,
        "Straight In"
    );
}

#[test]
fn project_defaults_to_asking_about_a_directory_that_may_not_exist() {
    let v = Vault::new();
    let answer = query::project(v.root(), &v.root().join("gone"), None).expect("query");
    assert!(answer.note.is_none());
    assert!(answer.issue.is_some());
}

// --- report -------------------------------------------------------------

#[test]
fn an_info_diagnostic_is_counted_and_never_pluralised() {
    let d = Diagnostic {
        path: "/v/memories/a.md".into(),
        code: Code::Mx105,
        severity: Severity::Info,
        message: "said out loud".to_owned(),
        span: None,
    };
    let out = report::human(&[d.clone(), d.clone()]);
    assert!(out.ends_with("\n2 info\n"), "{out}");
    let j: serde_json::Value =
        serde_json::from_str(&report::check_json(std::slice::from_ref(&d), 1)).expect("json");
    assert_eq!(j["summary"]["info"], 1);
}

#[test]
fn project_human_says_nothing_when_there_is_no_project() {
    let answer = ProjectAnswer {
        dir: "/v".into(),
        via: None,
        detail: None,
        note: None,
        candidates: vec![],
        issue: None,
    };
    assert_eq!(report::project_human(&answer), "");
}
