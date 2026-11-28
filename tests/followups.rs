//! `follow-ups`: the bullets a project's closed plans leave behind.

mod support;

use std::process::Command;

use assert_cmd::prelude::*;
use mnemex::followups::{self, Item};
use support::Vault;

fn texts(items: &[Item]) -> Vec<&str> {
    items.iter().map(|i| i.text.as_str()).collect()
}

/// A project owning `work`, so `follow-ups --for work/src` resolves to it.
fn owned_project(v: &Vault) -> String {
    v.dir("work/src");
    v.write(
        "projects",
        "202609061846-p",
        "---\nstatus: active\npath: work\n---\n\n# P\n\n## Purpose\n\npurpose\n",
    );
    "202609061846-p".to_owned()
}

fn plan(v: &Vault, id: &str, project: &str, status: &str, body: &str) {
    v.write(
        "plans",
        id,
        &format!("---\nproject: \"[[{project}]]\"\nstatus: {status}\n---\n\n# {id}\n\n{body}"),
    );
}

fn project_note(v: &Vault, id: &str) -> mnemex::query::NoteObject {
    mnemex::query::resolve(v.root(), id).expect("project")
}

// --- the section -------------------------------------------------------------

#[test]
fn a_bullet_carries_its_indented_continuation_lines() {
    let body = "# P\n\n## Follow-ups\n\n- the first bullet\n  wraps onto a second line\n- second\n";
    assert_eq!(
        texts(&followups::items(body)),
        ["the first bullet wraps onto a second line", "second"]
    );
}

#[test]
fn a_blank_line_inside_an_item_continues_it_when_the_next_line_is_indented() {
    let body = "## Follow-ups\n\n- first\n\n  more of it\n- second\n";
    assert_eq!(
        texts(&followups::items(body)),
        ["first more of it", "second"]
    );
}

#[test]
fn an_empty_or_prose_only_section_has_no_items() {
    assert_eq!(followups::items("## Follow-ups\n").len(), 0);
    assert_eq!(followups::items("## Follow-ups\n\nNone.\n").len(), 0);
    // Prose before the first bullet is not an item.
    assert_eq!(
        followups::items("## Follow-ups\n\nNothing else came up.\n").len(),
        0
    );
}

#[test]
fn a_plan_without_the_section_has_no_items() {
    let body = "# P\n\n## Goal\n\n- [ ] a goal, which is a step\n\n## Outcome\n\n- a note\n";
    assert_eq!(followups::items(body).len(), 0);
}

#[test]
fn the_section_ends_at_the_next_heading() {
    let body = "## Follow-ups\n\n- kept\n\n## Outcome\n\n- not a follow-up\n";
    assert_eq!(texts(&followups::items(body)), ["kept"]);
}

#[test]
fn the_section_runs_to_the_end_of_the_body() {
    let body = "## Follow-ups\n\n- last\n  continued\n";
    assert_eq!(texts(&followups::items(body)), ["last continued"]);
}

#[test]
fn a_heading_or_a_bullet_inside_a_fence_is_neither() {
    let body = "\
```
## Follow-ups
- fenced
```

## Follow-ups

- real

```
- also fenced
```

- after
";
    assert_eq!(texts(&followups::items(body)), ["real", "after"]);
}

#[test]
fn an_unindented_prose_line_ends_the_item_it_follows() {
    let body = "## Follow-ups\n\n- first\nprose between\n  ignored continuation\n- second\n";
    assert_eq!(texts(&followups::items(body)), ["first", "second"]);
}

#[test]
fn item_numbers_are_positions_in_the_section() {
    let items = followups::items("## Follow-ups\n\n- a\n- b\n- c\n");
    assert_eq!(
        items.iter().map(|i| i.n).collect::<Vec<_>>(),
        [1, 2, 3],
        "{items:?}"
    );
}

// --- the answer --------------------------------------------------------------

#[test]
fn only_closed_plans_with_an_item_are_listed() {
    let v = Vault::new();
    let p = owned_project(&v);
    plan(
        &v,
        "202609070816-open",
        &p,
        "open",
        "## Follow-ups\n\n- work beyond an open goal\n",
    );
    plan(
        &v,
        "202609070817-done",
        &p,
        "done",
        "## Follow-ups\n\n- closed work\n",
    );
    plan(
        &v,
        "202609070818-none",
        &p,
        "done",
        "## Follow-ups\n\nNone.\n",
    );
    let a = followups::run(v.root(), &project_note(&v, &p)).expect("run");
    assert_eq!(
        a.plans
            .iter()
            .map(|plan| plan.note.id.as_str())
            .collect::<Vec<_>>(),
        ["202609070817-done"]
    );
    assert_eq!(texts(&a.plans[0].items), ["closed work"]);
    assert_eq!(a.project.id, p);
}

#[test]
fn a_spoke_of_another_project_is_not_listed() {
    let v = Vault::new();
    let p = owned_project(&v);
    v.note("projects", "202609061847-other", "status: active\n");
    plan(
        &v,
        "202609070817-elsewhere",
        "202609061847-other",
        "done",
        "## Follow-ups\n\n- another project's work\n",
    );
    let a = followups::run(v.root(), &project_note(&v, &p)).expect("run");
    assert!(a.plans.is_empty(), "{:?}", a.plans);
}

#[test]
fn plans_tied_on_a_date_are_ordered_by_id() {
    let v = Vault::new();
    let p = owned_project(&v);
    // A vault that is not a repository dates both notes by mtime, which is the
    // same day, so only the id can order them.
    plan(&v, "202609070817-b", &p, "done", "## Follow-ups\n\n- b\n");
    plan(&v, "202609070816-a", &p, "done", "## Follow-ups\n\n- a\n");
    let a = followups::run(v.root(), &project_note(&v, &p)).expect("run");
    assert_eq!(
        a.plans
            .iter()
            .map(|plan| plan.note.id.as_str())
            .collect::<Vec<_>>(),
        ["202609070816-a", "202609070817-b"]
    );
}

/// Run `git -C <vault> <args>`, failing the test on a non-zero exit.
fn git(v: &Vault, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(v.root())
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

/// `git init` plus the identity a commit needs, or `false` when git is absent
/// or refuses, so a machine without it skips rather than fails.
fn git_vault(v: &Vault) -> bool {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: git is not on PATH");
        return false;
    }
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(v.root())
        .args(["init", "-q"])
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!("skipping: git init failed");
        return false;
    }
    git(v, &["config", "user.email", "a@b.c"]);
    git(v, &["config", "user.name", "fixture"]);
    true
}

#[test]
fn plans_are_ordered_newest_first_by_commit_date() {
    let v = Vault::new();
    let p = owned_project(&v);
    // The ids sort oldest first, so only the commit dates can order them.
    plan(&v, "202609070816-a", &p, "done", "## Follow-ups\n\n- a\n");
    plan(&v, "202609070817-b", &p, "done", "## Follow-ups\n\n- b\n");
    if !git_vault(&v) {
        return;
    }
    git(&v, &["add", "-A"]);
    git(
        &v,
        &[
            "commit",
            "-qm",
            "a",
            "--date=2020-01-02T00:00:00",
            "--",
            "plans/202609070816-a.md",
        ],
    );
    git(
        &v,
        &[
            "commit",
            "-qm",
            "b",
            "--date=2020-01-05T00:00:00",
            "--",
            "plans/202609070817-b.md",
        ],
    );
    let a = followups::run(v.root(), &project_note(&v, &p)).expect("run");
    assert_eq!(
        a.plans
            .iter()
            .map(|plan| plan.note.id.as_str())
            .collect::<Vec<_>>(),
        ["202609070817-b", "202609070816-a"]
    );
    assert_eq!(a.plans[0].changed, "2020-01-05", "{:?}", a.plans);
}

// --- the verb ----------------------------------------------------------------

fn mnemex(v: &Vault) -> Command {
    let mut c = Command::cargo_bin("mnemex").expect("binary");
    c.current_dir(v.root()).env("MNEMEX_VAULT", v.root());
    c
}

fn run(c: &mut Command) -> (i32, String, String) {
    let out = c.output().expect("run");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_verb_prints_every_item_in_full() {
    let v = Vault::new();
    let p = owned_project(&v);
    plan(
        &v,
        "202609070817-done",
        &p,
        "done",
        "## Follow-ups\n\n- `cargo deny` and advisory checks\n- a second bullet\n  that wraps\n",
    );
    let deep = v.root().join("work/src");
    let (code, out, _) = run(mnemex(&v).args(["follow-ups", "--for", &deep.display().to_string()]));
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("202609070817-done — 202609070817-done\n"),
        "{out}"
    );
    assert!(
        out.contains("  1. `cargo deny` and advisory checks\n"),
        "{out}"
    );
    assert!(out.contains("  2. a second bullet that wraps\n"), "{out}");
}

#[test]
fn a_project_with_no_follow_ups_exits_one() {
    let v = Vault::new();
    let _p = owned_project(&v);
    let deep = v.root().join("work/src");
    let (code, out, err) =
        run(mnemex(&v).args(["follow-ups", "--for", &deep.display().to_string()]));
    assert_eq!(code, 1, "{out}");
    assert_eq!(out, "");
    assert!(err.contains("no follow-ups"), "{err}");
}

#[test]
fn a_directory_no_project_owns_is_refused() {
    let v = Vault::new();
    let lonely = v.dir("lonely");
    let (code, _, err) =
        run(mnemex(&v).args(["follow-ups", "--for", &lonely.display().to_string()]));
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("no project owns"), "{err}");
}

#[test]
fn an_id_that_is_no_project_is_refused() {
    let v = Vault::new();
    let p = owned_project(&v);
    plan(
        &v,
        "202609070817-done",
        &p,
        "done",
        "## Follow-ups\n\n- x\n",
    );
    let (code, _, err) = run(mnemex(&v).args(["follow-ups", "--project", "202609079999-gone"]));
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("is no note"), "{err}");

    let (code, _, err) = run(mnemex(&v).args(["follow-ups", "--project", "202609070817-done"]));
    assert_eq!(code, 2, "{err}");
    assert!(
        err.contains("is a plan in `plans/`, not a project"),
        "{err}"
    );
}

#[test]
fn for_and_project_conflict() {
    let v = Vault::new();
    let _p = owned_project(&v);
    let (code, _, err) =
        run(mnemex(&v).args(["follow-ups", "--for", ".", "--project", "202609061846-p"]));
    assert_eq!(code, 2, "{err}");
}

#[test]
fn the_envelope_carries_the_items_and_the_project() {
    let v = Vault::new();
    let p = owned_project(&v);
    plan(
        &v,
        "202609070817-done",
        &p,
        "done",
        "## Follow-ups\n\n- first\n- second\n",
    );
    let (code, out, _) = run(mnemex(&v).args(["follow-ups", "--project", &p, "--json"]));
    assert_eq!(code, 0, "{out}");
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["version"], 1);
    assert_eq!(j["project"]["id"], p);
    assert_eq!(j["plans"][0]["id"], "202609070817-done");
    assert_eq!(j["plans"][0]["follow_ups"][0]["n"], 1);
    assert_eq!(j["plans"][0]["follow_ups"][0]["text"], "first");
    assert_eq!(j["plans"][0]["follow_ups"][1]["n"], 2);
}

#[test]
fn plans_are_separated_by_one_blank_line() {
    let v = Vault::new();
    let p = owned_project(&v);
    plan(&v, "202609070816-a", &p, "done", "## Follow-ups\n\n- a\n");
    plan(&v, "202609070817-b", &p, "done", "## Follow-ups\n\n- b\n");
    let (code, out, _) = run(mnemex(&v).args(["follow-ups", "--project", &p]));
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "202609070816-a — 202609070816-a\n  1. a\n\n202609070817-b — 202609070817-b\n  1. b\n"
    );
}
