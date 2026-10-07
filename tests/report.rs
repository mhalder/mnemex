//! The query and outcome renderings, driven through real notes.

mod support;

use mnemex::authoring::{self, Ctx, Outcome};
use mnemex::diagnostic::{Code, Diagnostic, Severity};
use mnemex::frontmatter::Position;
use mnemex::query;
use mnemex::report;
use support::Vault;

fn ctx(v: &Vault) -> Ctx<'_> {
    Ctx {
        root: v.root(),
        home: None,
        stamp: "202609081102",
    }
}

#[test]
fn resolve_and_list_renderings() {
    let v = Vault::new();
    v.write(
        "plans",
        "202609070816-x",
        "---\nproject: \"[[202609061846-p]]\"\nstatus: open\ntags:\n  - a\n---\n\n# The Plan\n",
    );
    v.note("projects", "202609061846-p", "status: active\n");

    let r = query::resolve(v.root(), "202609070816-x").expect("found");
    assert_eq!(
        report::resolve_human(&r),
        format!("plan: The Plan\n{}\n", r.path.display())
    );

    let j: serde_json::Value = serde_json::from_str(&report::resolve_json(Some(&r))).expect("json");
    assert_eq!(j["note"]["title"], "The Plan");
    assert_eq!(j["note"]["fields"]["tags"], serde_json::json!(["a"]));

    let notes = query::list(v.root(), None, None).expect("list");
    assert!(report::list_human(v.root(), &notes).contains("The Plan"));
    let j: serde_json::Value = serde_json::from_str(&report::list_json(&notes)).expect("json");
    assert_eq!(j["notes"][0]["id"], "202609061846-p");
}

#[test]
fn project_renderings_both_vias() {
    let v = Vault::new();
    v.dir("work/src");
    v.note("projects", "202609061846-p", "status: active\npath: work\n");
    let a = query::project(v.root(), &v.root().join("work/src"), None).expect("project");
    let human = report::project_human(&a);
    assert!(human.contains("202609061846-p — "), "{human}");
    assert!(human.contains("via path work"), "{human}");

    let j: serde_json::Value = serde_json::from_str(&report::project_json(&a)).expect("json");
    assert_eq!(j["via"], "path");
    assert_eq!(j["note"]["id"], "202609061846-p");
}

#[test]
fn show_renderings() {
    let v = Vault::new();
    v.note("projects", "202609061846-p", "status: active\n");
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
    let a = query::show(v.root(), "202609070816-x")
        .expect("show")
        .expect("found");
    let human = report::show_human(&a);
    assert!(human.contains("plan/202609070816-x — "), "{human}");
    assert!(human.contains("(open)"), "{human}");
    assert!(human.contains("project: project/202609061846-p"), "{human}");
    assert!(human.contains("adr/202609061400-a — "), "{human}");

    let j: serde_json::Value = serde_json::from_str(&report::show_json(&a)).expect("json");
    assert_eq!(j["project"]["id"], "202609061846-p");
    assert_eq!(j["spokes"]["adrs"][0]["id"], "202609061400-a");
}

#[test]
fn show_reports_a_dangling_project() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609079999-gone]]\"\nstatus: open\n",
    );
    let a = query::show(v.root(), "202609070816-x")
        .expect("show")
        .expect("found");
    let human = report::show_human(&a);
    assert!(
        human.contains("202609079999-gone (no such note)"),
        "{human}"
    );
    let j: serde_json::Value = serde_json::from_str(&report::show_json(&a)).expect("json");
    assert_eq!(j["project"]["note"], serde_json::Value::Null);
}

#[test]
fn an_outcome_reports_its_notes_and_diagnostics() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:owner/mnemex.git");
    let out = authoring::new(
        ctx(&v),
        mnemex::kind::Kind::Project,
        "P",
        authoring::NewArgs {
            path: Some("work"),
            ..authoring::NewArgs::default()
        },
    )
    .expect("new");
    let outcome = Outcome {
        headline: "created x".to_owned(),
        path: out.path.clone(),
        notes: vec!["a note".to_owned()],
        diagnostics: vec![Diagnostic {
            path: out.path.clone(),
            code: Code::Mx105,
            severity: Severity::Warning,
            message: "warn".to_owned(),
            span: Some(Position::new(1, 1)),
        }],
    };
    let human = report::outcome_human(&outcome);
    assert!(human.contains("created x"), "{human}");
    assert!(human.contains("a note"), "{human}");
    assert!(human.contains("warning[MX105]"), "{human}");
}

#[test]
fn a_fields_json_with_an_unsupported_value_is_null() {
    let v = Vault::new();
    v.write(
        "plans",
        "202609070816-x",
        "---\nproject: \"[[202609061846-p]]\"\nnested:\n  a: 1\n---\n\n# X\n",
    );
    v.note("projects", "202609061846-p", "status: active\n");
    let r = query::resolve(v.root(), "202609070816-x").expect("found");
    let j: serde_json::Value = serde_json::from_str(&report::resolve_json(Some(&r))).expect("json");
    assert_eq!(j["note"]["fields"]["nested"], serde_json::Value::Null);
}
