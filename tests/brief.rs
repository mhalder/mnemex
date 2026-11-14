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
    assert!(human.contains("202609061846-p · work · no repo"), "{human}");
    // The body already ends with a newline, so the section adds exactly one.
    assert!(human.contains("the purpose\n\n## Open plans"), "{human}");
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
fn a_lone_step_is_counted_in_the_singular() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(
        &v,
        "plans",
        "202609070816-x",
        &p,
        "status: open\n",
        "X",
        "- [ ] the only step\n",
    );
    // A plan whose one step is done is still listed: the noun follows the total.
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
    let human = &b.human;
    assert!(human.contains("(1 of 1 step open)"), "{human}");
    assert!(human.contains("(0 of 1 step open)"), "{human}");
    assert!(
        b.next
            .iter()
            .any(|n| n.ends_with("before starting: 1 open step")),
        "{:?}",
        b.next
    );
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

#[test]
fn brief_json_carries_the_rendered_text_of_the_same_run() {
    let v = Vault::new();
    let p = owned_project(&v, "the purpose");
    write_spoke(
        &v,
        "plans",
        "202609070816-x",
        &p,
        "status: open\n",
        "X",
        "- [ ] a\n",
    );
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    let j: serde_json::Value = serde_json::from_str(&brief::json(&b)).expect("json");
    // One run serves both readers: the text a session injects and the lists a
    // display shows cannot disagree, as two separate runs could.
    assert_eq!(j["text"], b.human.as_str());
    assert!(
        b.human.contains("the purpose"),
        "the text is the full brief"
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
fn a_git_backed_vault_dates_each_note_by_its_commit_and_orders_memories_newest_first() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    // The ids sort oldest first, so only the commit dates can order them.
    write_spoke(&v, "memories", "202609081100-a", &p, "", "A", "");
    write_spoke(&v, "memories", "202609081101-b", &p, "", "B", "");
    // A ten-character name with a `-` at index 4 is not a date, and git lists
    // it first, so reading it as one would date every note after it.
    v.raw("", "abcd-ef.md", "not a date\n");
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
            "abcd-ef.md",
            "memories/202609081100-a.md",
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
            "memories/202609081101-b.md",
        ],
    );

    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    // The commit date, not the file's mtime, which is today.
    assert_eq!(
        b.changed.get("202609081100-a").map(String::as_str),
        Some("2020-01-02"),
        "{:?}",
        b.changed
    );
    assert_eq!(
        b.changed.get("202609081101-b").map(String::as_str),
        Some("2020-01-05"),
        "{:?}",
        b.changed
    );
    let ids: Vec<&str> = b.memories.iter().map(|m| m.note.id.as_str()).collect();
    assert_eq!(ids, ["202609081101-b", "202609081100-a"], "newest first");
    // The rendered list carries the dates, newest first.
    let older = b.human.find("202609081100-a").expect("a listed");
    let newer = b.human.find("202609081101-b").expect("b listed");
    assert!(newer < older, "{}", b.human);
}

#[test]
fn a_vault_that_is_not_a_repository_dates_a_note_by_its_mtime() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    write_spoke(&v, "memories", "202609081100-m", &p, "", "M", "");
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    let changed = b.changed.get("202609081100-m").expect("dated");
    let b10 = changed.as_bytes();
    assert_eq!(b10.len(), 10, "{changed}");
    assert!(
        b10[4] == b'-'
            && b10[7] == b'-'
            && b10
                .iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit()),
        "{changed}"
    );
}

#[test]
fn the_budget_accounts_for_every_body_it_includes() {
    let v = Vault::new();
    let p = owned_project(&v, "");
    for id in ["202609081100-a", "202609081101-b"] {
        write_spoke(
            &v,
            "contexts",
            id,
            &p,
            "",
            "C",
            &format!("{}\n", "x".repeat(2000)),
        );
    }
    // Smaller than the product the budget arithmetic could make of one body,
    // larger than the two bodies it must fit.
    let b = brief_for(&v, 5000);
    let included = b.contexts.iter().filter(|c| c.body_included).count();
    assert_eq!(included, 2, "{:?}", b.contexts);
    assert_eq!(
        b.human.matches(&"x".repeat(2000)).count(),
        2,
        "both bodies are rendered"
    );
}

#[test]
fn the_project_body_loses_its_heading_line_and_nothing_else() {
    let v = Vault::new();
    v.dir("work/src");
    v.write(
        "projects",
        "202609061846-p",
        "---\nstatus: active\npath: work\n---\n\n# P\n\n## Purpose\n\nfirst line\n",
    );
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    // The title line is dropped, and nothing else is: the body keeps both of
    // its blank lines, so a dropped line shows up as one fewer newline.
    assert!(!b.human.contains("# P\n"), "{}", b.human);
    assert!(
        b.human
            .contains("no repo\n\n\n\n## Purpose\n\nfirst line\n"),
        "{}",
        b.human
    );
}

#[test]
fn the_next_block_is_rendered_when_it_has_lines() {
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
    let b = brief_for(&v, brief::DEFAULT_BUDGET);
    assert!(b.human.contains("\n## Next\n"), "{}", b.human);
    assert!(
        b.human
            .contains("- read plans/202609070816-x.md before starting: 1 open step\n"),
        "{}",
        b.human
    );
}
