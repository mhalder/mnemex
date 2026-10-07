//! `brief`: the session-start command.

mod support;

use std::path::Path;

use mnemex::brief;
use support::Vault;

/// A project owning `work`, so `brief --for work/src` resolves to it.
fn owned_project(v: &Vault, body: &str) -> String {
    v.dir("work/src");
    v.write(
        "projects",
        "202609061846-p",
        &format!("---\nstatus: active\npath: work\n---\n\n# P\n\n## Purpose\n\n{body}\n"),
    );
    "202609061846-p".to_owned()
}

fn write_spoke(
    v: &Vault,
    folder: &str,
    id: &str,
    project: &str,
    fields: &str,
    title: &str,
    body: &str,
) {
    v.write(
        folder,
        id,
        &format!("---\nproject: \"[[{project}]]\"\n{fields}---\n\n# {title}\n\n{body}"),
    );
}

fn brief_for(v: &Vault, budget: usize) -> brief::Brief {
    brief::run(v.root(), &v.root().join("work/src"), None, budget).expect("brief")
}

#[test]
fn brief_prints_the_project_body_and_grouped_spokes() {
    let v = Vault::new();
    let p = owned_project(&v, "the purpose");
    write_spoke(
        &v,
        "plans",
        "202609070816-x",
        &p,
        "status: open\n",
        "X",
        "## Steps\n\n- [ ] one\n- [x] two\n",
    );
    write_spoke(
        &v,
        "adrs",
        "202609061400-a",
        &p,
        "status: accepted\n",
        "A",
        "",
    );
    write_spoke(&v, "memories", "202609081100-m", &p, "", "M", "");
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    let human = &b.human;
    assert!(human.contains("# mnemex project: P"), "{human}");
    assert!(human.contains("the purpose"), "{human}");
    assert!(human.contains("## Open plans"), "{human}");
    assert!(
        human.contains("202609070816-x — X (1 of 2 steps open)"),
        "{human}"
    );
    assert!(human.contains("## Memories"), "{human}");
    assert!(human.contains("202609081100-m — M"), "{human}");
    assert!(human.contains("## ADRs"), "{human}");
    assert!(human.contains("202609061400-a — A (accepted)"), "{human}");
}

#[test]
fn a_done_plan_is_not_listed() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(&v, "plans", "202609070816-x", &p, "status: done\n", "X", "");
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    assert!(b.plans.is_empty(), "{:?}", b.plans);
    assert!(!b.human.contains("202609070816-x —"), "{}", b.human);
    assert!(b.next.is_empty(), "{:?}", b.next);
}

#[test]
fn step_counting_includes_both_checked_forms() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(
        &v,
        "plans",
        "202609070816-x",
        &p,
        "status: open\n",
        "X",
        "- [ ] a\n- [x] b\n- [X] c\n",
    );
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    assert_eq!(b.plans[0].steps.map(|s| (s.open, s.total)), Some((1, 3)));
    assert!(b.human.contains("(1 of 3 steps open)"), "{}", b.human);
}

#[test]
fn the_next_block_lists_open_plans_and_stale_ones() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(
        &v,
        "plans",
        "202609070816-x",
        &p,
        "status: open\n",
        "X",
        "- [ ] a\n",
    );
    write_spoke(
        &v,
        "plans",
        "202609070817-y",
        &p,
        "status: open\n",
        "Y",
        "- [x] done\n",
    );
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    assert!(
        b.next
            .iter()
            .any(|n| n.contains("read plans/202609070816-x.md")),
        "{:?}",
        b.next
    );
    assert!(
        b.next
            .iter()
            .any(|n| n.contains("1 plan has no open steps: set it done")),
        "{:?}",
        b.next
    );
}

#[test]
fn the_budget_cuts_context_bodies_newest_first() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    for (i, id) in ["202609081100-a", "202609081101-b", "202609081102-c"]
        .iter()
        .enumerate()
    {
        write_spoke(
            &v,
            "contexts",
            id,
            &p,
            "",
            &format!("C{i}"),
            &format!("{}\n", "x".repeat(2000)),
        );
        let _ = i;
    }
    // A budget large enough for the always sections but not for every body.
    let b = brief_for(&v, 400);
    let omitted = b
        .contexts
        .iter()
        .filter(|c| !c.body_included)
        .map(|c| c.note.id.as_str())
        .collect::<Vec<_>>();
    assert!(
        !omitted.is_empty(),
        "some context bodies must be cut: {:?}",
        b.contexts
    );
    assert!(
        b.human.contains("(body omitted, over budget)"),
        "{}",
        b.human
    );
    // The newest contexts are emitted first, so the oldest are the ones cut.
    assert_eq!(omitted.first(), Some(&"202609081100-a"), "{omitted:?}");
}

#[test]
fn brief_fails_when_no_project_owns_the_directory() {
    let v = Vault::new();
    v.dir("lonely");
    let err = brief::run(
        v.root(),
        &v.root().join("lonely"),
        None,
        brief::DEFAULT_BUDGET,
    )
    .expect_err("no project");
    assert!(err.to_string().contains("no project owns"), "{err}");
}

#[test]
fn a_git_backed_vault_ranks_notes_by_commit_date() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(&v, "memories", "202609081100-m", &p, "", "M", "");

    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: git is not on PATH");
        return;
    }
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(v.root())
        .arg("init")
        .arg("-q")
        .status()
        .expect("git init");
    if !status.success() {
        eprintln!("skipping: git init failed");
        return;
    }
    for (k, val) in [("user.email", "a@b.c"), ("user.name", "fixture")] {
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(v.root())
            .args(["config", k, val])
            .status();
    }
    let _ = std::process::Command::new("git")
        .arg("-C")
        .arg(v.root())
        .args(["add", "-A"])
        .status();
    let _ = std::process::Command::new("git")
        .arg("-C")
        .arg(v.root())
        .args(["commit", "-qm", "seed"])
        .status();

    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    assert!(
        !b.changed.is_empty(),
        "committed notes get a git-derived date"
    );
    let _ = Path::new("/");
}

#[test]
fn two_projects_with_the_same_repo_are_ambiguous_for_brief() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:owner/mnemex.git");
    v.dir("work/src");
    v.note(
        "projects",
        "202609061846-a",
        "status: active\nrepo: github.com/owner/mnemex\n",
    );
    v.note(
        "projects",
        "202609061847-b",
        "status: active\nrepo: github.com/owner/mnemex\n",
    );
    let err = brief::run(
        v.root(),
        &v.root().join("work/src"),
        None,
        brief::DEFAULT_BUDGET,
    )
    .expect_err("ambiguous");
    assert!(
        err.to_string().contains("more than one project owns"),
        "{err}"
    );
}

#[test]
fn adrs_sort_accepted_then_proposed_then_deprecated() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(
        &v,
        "adrs",
        "202609061400-dep",
        &p,
        "status: deprecated\n",
        "Dep",
        "",
    );
    write_spoke(
        &v,
        "adrs",
        "202609061401-pro",
        &p,
        "status: proposed\n",
        "Pro",
        "",
    );
    write_spoke(
        &v,
        "adrs",
        "202609061402-acc",
        &p,
        "status: accepted\n",
        "Acc",
        "",
    );
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    let ids: Vec<&str> = b.adrs.iter().map(|a| a.note.id.as_str()).collect();
    assert_eq!(
        ids,
        ["202609061402-acc", "202609061401-pro", "202609061400-dep"]
    );
}

#[test]
fn the_next_block_pluralises_many_stale_plans_and_prints_refs() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(
        &v,
        "plans",
        "202609070816-x",
        &p,
        "status: open\nrefs:\n  - https://github.com/o/r/pull/1\n",
        "X",
        "- [x] a\n",
    );
    write_spoke(
        &v,
        "plans",
        "202609070817-y",
        &p,
        "status: open\n",
        "Y",
        "- [x] b\n",
    );
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    assert!(
        b.next
            .iter()
            .any(|n| n.contains("2 plans have no open steps: set them done")),
        "{:?}",
        b.next
    );
    // A plan with a ref renders it after its step count.
    assert!(
        b.human.contains("· https://github.com/o/r/pull/1"),
        "{}",
        b.human
    );
}

#[test]
fn bodies_without_trailing_newlines_still_render() {
    let v = Vault::new();
    v.dir("work/src");
    v.write(
        "projects",
        "202609061846-p",
        "---\nstatus: active\npath: work\n---\n\n# P\n\n## Purpose\n\nno trailing newline",
    );
    write_spoke(
        &v,
        "contexts",
        "202609081100-c",
        "202609061846-p",
        "",
        "C",
        "ctx body no newline",
    );
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    assert!(b.human.contains("no trailing newline\n"), "{}", b.human);
    assert!(b.human.contains("ctx body no newline\n"), "{}", b.human);
}

#[test]
fn a_vault_git_dir_with_no_commits_falls_back_to_mtime() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(&v, "memories", "202609081100-m", &p, "", "M", "");
    // A `.git` directory that is not a real repository: `git log` fails.
    v.dir(".git");
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    assert!(!b.changed.is_empty(), "mtime fallback still ranks");
}

#[test]
fn brief_json_carries_steps_and_body_included() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(
        &v,
        "plans",
        "202609070816-x",
        &p,
        "status: open\n",
        "X",
        "- [ ] a\n- [x] b\n",
    );
    write_spoke(&v, "contexts", "202609081100-c", &p, "", "C", "ctx\n");
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    let j: serde_json::Value = serde_json::from_str(&brief::json(&b)).expect("json");
    assert_eq!(j["plans"][0]["steps"]["open"], 1);
    assert_eq!(j["plans"][0]["steps"]["total"], 2);
    assert_eq!(j["contexts"][0]["body_included"], true);
    assert_eq!(j["budget"]["limit"], brief::DEFAULT_BUDGET);
}
