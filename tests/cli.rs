//! The command surface, its exit codes, and the documented output shapes.

mod support;

use std::io::Write as _;
use std::process::Command;

use assert_cmd::prelude::*;
use support::Vault;

fn mnemex(v: &Vault) -> Command {
    let mut c = Command::cargo_bin("mnemex").expect("binary");
    c.current_dir(v.root()).env("MNEMEX_VAULT", v.root());
    c
}

/// Run and return `(code, stdout, stderr)`.
fn run(c: &mut Command) -> (i32, String, String) {
    let out = c.output().expect("run");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn project(v: &Vault, id: &str) {
    v.note("projects", id, "status: active\n");
}

// --- exit codes --------------------------------------------------------------

#[test]
fn a_clean_check_exits_zero_with_the_trailer() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let (code, out, _) = run(mnemex(&v).args(["check"]));
    assert_eq!(out, "clean, 1 note\n", "{out}");
    assert_eq!(code, 0);
}

#[test]
fn errors_present_exit_one() {
    let v = Vault::new();
    v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    project(&v, "202609061846-p");
    let (code, out, _) = run(mnemex(&v).args(["check"]));
    assert!(out.contains("error[MX102]"), "{out}");
    assert!(out.contains("1 error in 2 notes"), "{out}");
    assert_eq!(code, 1);
}

#[test]
fn warnings_alone_exit_zero() {
    let v = Vault::new();
    v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: accepted\nowner: me\n",
    );
    project(&v, "202609061846-p");
    let (code, out, _) = run(mnemex(&v).args(["check"]));
    assert!(out.contains("warning[MX105]"), "{out}");
    assert_eq!(code, 0);
}

#[test]
fn a_query_that_finds_nothing_exits_one_and_explains_on_stderr() {
    let v = Vault::new();
    let (code, out, err) = run(mnemex(&v).args(["resolve", "202609079999-gone"]));
    assert_eq!(out, "");
    assert!(err.contains("202609079999-gone"), "{err}");
    assert_eq!(code, 1);
}

#[test]
fn no_vault_at_the_resolved_path_exits_two() {
    let v = Vault::bare();
    let missing = v.root().join("no-such-dir");
    let mut c = Command::cargo_bin("mnemex").expect("binary");
    c.args(["check"]).env("MNEMEX_VAULT", &missing);
    let (code, _, err) = run(&mut c);
    assert_eq!(code, 2);
    assert!(err.contains("no vault at"), "{err}");
}

// --- authoring through the binary -------------------------------------------

#[test]
fn new_project_and_a_spoke_round_trip() {
    let v = Vault::new();
    let (code, out, _) = run(mnemex(&v).args(["new", "project", "mnemex"]));
    assert_eq!(code, 0);
    let id = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    assert!(id.ends_with("-mnemex"));

    let (code, out, _) =
        run(mnemex(&v).args(["new", "plan", "Write enforcement", "--project", id]));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("created "), "{out}");
}

#[test]
fn new_requires_project_for_a_spoke_and_refuses_it_for_a_project() {
    let v = Vault::new();
    let (code, _, err) = run(mnemex(&v).args(["new", "plan", "T"]));
    assert_eq!(code, 2);
    assert!(err.contains("needs `--project <project-id>`"), "{err}");

    let (code, _, err) =
        run(mnemex(&v).args(["new", "project", "P", "--project", "202609061846-x"]));
    assert_eq!(code, 2);
    assert!(err.contains("does not apply"), "{err}");
}

#[test]
fn set_and_delete_a_spoke() {
    let v = Vault::new();
    let (code, out, _) = run(mnemex(&v).args(["new", "project", "P"]));
    assert_eq!(code, 0);
    let pid = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    let (code, out, _) = run(mnemex(&v).args(["new", "memory", "M", "--project", pid]));
    assert_eq!(code, 0);
    let mid = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");

    let (code, _, _) = run(mnemex(&v).args(["set", mid, "tags", "a", "b"]));
    assert_eq!(code, 0);

    let (code, _, _) = run(mnemex(&v).args(["delete", mid]));
    assert_eq!(code, 0, "a spoke deletes outright");
}

#[test]
fn delete_refuses_a_project_that_spokes_name() {
    let v = Vault::new();
    let (code, out, _) = run(mnemex(&v).args(["new", "project", "P"]));
    assert_eq!(code, 0);
    let pid = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    let (code, out, _) = run(mnemex(&v).args(["new", "plan", "T", "--project", pid]));
    assert_eq!(code, 0);
    let _ = out;

    let (code, _, err) = run(mnemex(&v).args(["delete", pid]));
    assert_eq!(code, 2);
    assert!(err.contains("is named in `project` by"), "{err}");
}

#[test]
fn set_plan_status_done_closes_a_plan() {
    let v = Vault::new();
    let (code, out, _) = run(mnemex(&v).args(["new", "project", "P"]));
    assert_eq!(code, 0);
    let pid = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    let (code, out, _) = run(mnemex(&v).args(["new", "plan", "T", "--project", pid]));
    assert_eq!(code, 0);
    let plan_id = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");

    let (code, _, _) = run(mnemex(&v).args(["set", plan_id, "status", "done"]));
    assert_eq!(code, 0);
    let src = std::fs::read_to_string(v.root().join(format!("plans/{plan_id}.md"))).expect("read");
    assert!(src.contains("status: done"), "{src}");
}

// --- queries through the binary ---------------------------------------------

#[test]
fn project_for_answers_by_path() {
    let v = Vault::new();
    v.dir("work/src");
    v.note("projects", "202609061846-p", "status: active\npath: work\n");
    let deep = v.root().join("work/src");
    let (code, out, _) = run(mnemex(&v).args(["project", "--for", &deep.display().to_string()]));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("202609061846-p — "), "{out}");
    assert!(out.contains("via path work"), "{out}");
}

#[test]
fn list_project_keeps_only_the_spokes() {
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
    v.note(
        "memories",
        "202609081100-m",
        "project: \"[[202609061847-q]]\"\n",
    );
    project(&v, "202609061847-q");
    let (code, out, _) = run(mnemex(&v).args(["list", "--project", "202609061846-p"]));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("202609070816-x"), "{out}");
    assert!(out.contains("202609061400-a"), "{out}");
    assert!(!out.contains("202609081100-m"), "{out}");
}

#[test]
fn show_and_resolve_emit_the_note_object() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let (code, out, _) = run(mnemex(&v).args(["resolve", "202609070816-x", "--json"]));
    assert_eq!(code, 0);
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["note"]["id"], "202609070816-x");
    assert_eq!(j["note"]["fields"]["status"], "open");

    let (code, out, _) = run(mnemex(&v).args(["show", "202609070816-x", "--json"]));
    assert_eq!(code, 0);
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["project"]["id"], "202609061846-p");
    assert_eq!(j["spokes"]["plans"][0]["id"], "202609070816-x");
}

#[test]
fn brief_prints_the_project_body_and_open_plans() {
    let v = Vault::new();
    v.write(
        "projects",
        "202609061846-p",
        "---\nstatus: active\npath: work\n---\n\n# P\n\n## Purpose\n\nsome purpose\n",
    );
    v.dir("work/src");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let deep = v.root().join("work/src");
    let (code, out, _) = run(mnemex(&v).args(["brief", "--for", &deep.display().to_string()]));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("# mnemex project: P"), "{out}");
    assert!(out.contains("some purpose"), "{out}");
    assert!(out.contains("## Open plans"), "{out}");
    assert!(out.contains("202609070816-x — "), "{out}");
}

// --- the rest of the surface ------------------------------------------------

#[test]
fn schema_human_and_json_are_the_same_table() {
    let v = Vault::new();
    let (code, out, _) = run(mnemex(&v).args(["schema"]));
    assert_eq!(code, 0);
    assert!(out.contains("project (projects/)"), "{out}");
    let (code, out, _) = run(mnemex(&v).args(["schema", "--json"]));
    assert_eq!(code, 0);
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["kinds"][0]["kind"], "project");
}

#[test]
fn a_relative_path_flag_is_taken_from_the_working_directory() {
    let v = Vault::new();
    let work = v.dir("somewhere");
    let mut c = mnemex(&v);
    c.current_dir(v.root());
    let (code, out, _) = run(c.args(["new", "project", "P", "--path", "somewhere"]));
    assert_eq!(code, 0, "{out}");
    let id = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    let src = std::fs::read_to_string(v.root().join(format!("projects/{id}.md"))).expect("read");
    assert!(src.contains(&format!("path: {}", work.display())), "{src}");
}

#[test]
fn set_path_derives_repo_from_a_checkout() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:owner/mnemex.git");
    let (code, out, _) = run(mnemex(&v).args(["new", "project", "P"]));
    assert_eq!(code, 0);
    let id = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    let (code, out, _) = run(mnemex(&v).args(["set", id, "path", "work"]));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("`repo` is `github.com/owner/mnemex`"), "{out}");
}

#[test]
fn the_hook_blocks_a_bad_write_and_is_silent_on_a_good_one() {
    let v = Vault::new();
    v.note("projects", "202609061846-p", "status: active\n");
    let bad = v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    let payload = serde_json::json!({ "tool_input": { "file_path": bad } }).to_string();
    let mut c = mnemex(&v);
    c.args(["hook"]);
    let mut child = c
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("MX102"));

    let good = v.note(
        "memories",
        "202609081100-m",
        "project: \"[[202609061846-p]]\"\n",
    );
    let payload = serde_json::json!({ "tool_input": { "file_path": good } }).to_string();
    let mut c = mnemex(&v);
    c.args(["hook"]);
    let mut child = c
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
}

#[test]
fn resolve_and_list_human_outputs() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let (code, out, _) = run(mnemex(&v).args(["resolve", "202609070816-x"]));
    assert_eq!(code, 0);
    assert!(out.starts_with("plan: "), "{out}");

    let (code, out, _) = run(mnemex(&v).args(["list"]));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("202609070816-x"), "{out}");

    let (code, out, _) = run(mnemex(&v).args(["list", "--json"]));
    assert_eq!(code, 0);
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["notes"][0]["id"], "202609061846-p");
}

#[test]
fn show_human_and_json() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let (code, out, _) = run(mnemex(&v).args(["show", "202609070816-x"]));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("project: project/202609061846-p"), "{out}");
    assert!(out.contains("(open)"), "{out}");
}

#[test]
fn project_ambiguous_and_none_exit_one_with_candidates_or_message() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:owner/mnemex.git");
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
    let deep = v.root().join("work/src");
    v.dir("work/src");
    let (code, _out, err) = run(mnemex(&v).args(["project", "--for", &deep.display().to_string()]));
    assert_eq!(code, 1);
    assert!(err.contains("202609061846-a"), "{err}");
    assert!(err.contains("202609061847-b"), "{err}");
    let (code, out, _) =
        run(mnemex(&v).args(["project", "--for", &deep.display().to_string(), "--json"]));
    assert_eq!(code, 1);
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["note"], serde_json::Value::Null);
    assert_eq!(j["candidates"].as_array().expect("array").len(), 2);

    let lonely = v.dir("lonely");
    let (code, _, err) = run(mnemex(&v).args(["project", "--for", &lonely.display().to_string()]));
    assert_eq!(code, 1);
    assert!(err.contains("no project owns"), "{err}");
}

#[test]
fn check_path_and_check_json() {
    let v = Vault::new();
    let note = v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    project(&v, "202609061846-p");
    let (code, out, _) = run(mnemex(&v).args(["check", "--path", &note.display().to_string()]));
    assert_eq!(code, 1);
    assert!(out.contains("1 error in 1 note"), "{out}");

    let (code, out, _) = run(mnemex(&v).args(["check", "--json"]));
    assert_eq!(code, 1);
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["summary"]["error"], 1);
}

#[test]
fn adopt_rename_and_delete_through_the_binary() {
    let v = Vault::new();
    let (code, out, _) = run(mnemex(&v).args(["new", "project", "P"]));
    assert_eq!(code, 0);
    let pid = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");

    let stray = v.root().join("draft.md");
    std::fs::write(&stray, "# Draft\n\nbody\n").expect("write");
    let (code, out, _) = run(mnemex(&v).args([
        "adopt",
        &stray.display().to_string(),
        "--kind",
        "plan",
        "--project",
        pid,
    ]));
    assert_eq!(code, 0, "{out}");
    let plan_id = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");

    let (code, out, _) = run(mnemex(&v).args(["rename", plan_id, "Renamed"]));
    assert_eq!(code, 0, "{out}");
    let new_id = format!("{}-renamed", &plan_id[..12]);

    let (code, _, _) = run(mnemex(&v).args(["delete", &new_id]));
    assert_eq!(code, 0);

    let (code, _, err) = run(mnemex(&v).args(["delete", pid]));
    assert_eq!(code, 0, "{err}"); // the spoke is gone, so the project deletes
}

#[test]
fn set_clear_and_unknown_kind() {
    let v = Vault::new();
    let (code, out, _) = run(mnemex(&v).args(["new", "project", "P"]));
    assert_eq!(code, 0);
    let pid = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    let (code, _out, _) = run(mnemex(&v).args(["set", pid, "tags", "a"]));
    assert_eq!(code, 0);
    let (code, _, _) = run(mnemex(&v).args(["set", pid, "tags", "--clear"]));
    assert_eq!(code, 0);

    let (code, _, err) = run(mnemex(&v).args(["new", "bogus", "T"]));
    assert_eq!(code, 2);
    assert!(err.contains("is not a kind"), "{err}");
}

#[test]
fn brief_json_and_no_project() {
    let v = Vault::new();
    v.write(
        "projects",
        "202609061846-p",
        "---\nstatus: active\npath: work\n---\n\n# P\n\n## Purpose\n\nx\n",
    );
    v.dir("work/src");
    let deep = v.root().join("work/src");
    let (code, out, _) =
        run(mnemex(&v).args(["brief", "--for", &deep.display().to_string(), "--json"]));
    assert_eq!(code, 0);
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["project"]["id"], "202609061846-p");
    assert_eq!(j["budget"]["limit"], 16384);

    let lonely = v.dir("lonely");
    let (code, _, err) = run(mnemex(&v).args(["brief", "--for", &lonely.display().to_string()]));
    assert_eq!(code, 1);
    assert!(err.contains("no project owns"), "{err}");
}

#[test]
fn absolute_and_dot_path_spellings_are_kept_or_resolved() {
    let v = Vault::new();
    let abs = v.root().join("absdir");
    std::fs::create_dir_all(&abs).expect("mkdir");
    // An absolute path is taken as given.
    let (code, out, _) =
        run(mnemex(&v).args(["new", "project", "P", "--path", &abs.display().to_string()]));
    assert_eq!(code, 0, "{out}");
    let id = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    let src = std::fs::read_to_string(v.root().join(format!("projects/{id}.md"))).expect("read");
    assert!(src.contains(&format!("path: {}", abs.display())), "{src}");

    // `~/` stays as written.
    let (code, out, _) = run(mnemex(&v).args(["new", "project", "Q", "--path", "~/q"]));
    assert_eq!(code, 0, "{out}");
    let id = out
        .lines()
        .next()
        .expect("headline")
        .trim_start_matches("created ");
    let src = std::fs::read_to_string(v.root().join(format!("projects/{id}.md"))).expect("read");
    assert!(src.contains("path: ~/q"), "{src}");
}

#[test]
fn rename_dry_run_and_show_absences() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let (code, out, _) = run(mnemex(&v).args(["rename", "202609070816-x", "New", "--dry-run"]));
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("would rename"), "{out}");

    // show on an unparseable note exits 1 and names the check that explains it.
    let broken = v.write("plans", "202609070817-b", "---\nproject: [\n---\n\n# B\n");
    let (code, out, err) = run(mnemex(&v).args(["show", "202609070817-b"]));
    assert_eq!(code, 1);
    assert_eq!(out, "");
    assert!(err.contains("its frontmatter does not parse"), "{err}");

    // show on an unknown id emits a null envelope with --json.
    let (code, out, _) = run(mnemex(&v).args(["show", "202609079999-gone", "--json"]));
    assert_eq!(code, 1);
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["note"], serde_json::Value::Null);
    let _ = broken;
}

#[test]
fn an_empty_list_says_no_notes() {
    let v = Vault::new();
    let (code, _, err) = run(mnemex(&v).args(["list"]));
    assert_eq!(code, 1);
    assert!(err.contains("no notes in"), "{err}");
}

#[test]
fn a_list_that_matches_nothing_names_the_filter() {
    let v = Vault::new();
    project(&v, "202609061846-p");

    // A project filter that matches nothing must not read as an empty vault.
    let (code, _, err) = run(mnemex(&v).args(["list", "--project", "202609069999-gone"]));
    assert_eq!(code, 1);
    assert!(
        err.contains("no notes for project `202609069999-gone` in"),
        "{err}"
    );

    // So must a kind filter.
    let (code, _, err) = run(mnemex(&v).args(["list", "--kind", "plan"]));
    assert_eq!(code, 1);
    assert!(err.contains("no notes of kind `plan` in"), "{err}");

    // And both together.
    let (code, _, err) =
        run(mnemex(&v).args(["list", "--kind", "plan", "--project", "202609061846-p"]));
    assert_eq!(code, 1);
    assert!(
        err.contains("no notes of kind `plan` for project `202609061846-p` in"),
        "{err}"
    );

    // A real project's slug is called out with the id to use instead.
    let (code, _, err) = run(mnemex(&v).args(["list", "--project", "p"]));
    assert_eq!(code, 1);
    assert!(err.contains("no notes for project `p` in"), "{err}");
    assert!(
        err.contains("`p` is a slug, not an id; use `202609061846-p`"),
        "{err}"
    );
}

#[test]
fn the_hook_debug_branch_reports_reasons() {
    let v = Vault::new();
    let p = v.write("daily", "x", "not a note\n");
    let payload = serde_json::json!({ "tool_input": { "file_path": p } }).to_string();
    let mut c = mnemex(&v);
    c.args(["hook"]).env("MNEMEX_HOOK_DEBUG", "1");
    let mut child = c
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("mnemex hook:"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_hook_with_no_vault_is_silent() {
    let payload = serde_json::json!({ "tool_input": { "file_path": "/x/y.md" } }).to_string();
    let mut c = Command::cargo_bin("mnemex").expect("binary");
    c.args(["hook"])
        .env_remove("MNEMEX_VAULT")
        .env_remove("HOME");
    let mut child = c
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
}

#[test]
fn a_hook_with_no_vault_and_debug_on_reports_the_reason() {
    let payload = serde_json::json!({ "tool_input": { "file_path": "/x/y.md" } }).to_string();
    let mut c = Command::cargo_bin("mnemex").expect("binary");
    c.args(["hook"])
        .env_remove("MNEMEX_VAULT")
        .env_remove("HOME")
        .env("MNEMEX_HOOK_DEBUG", "1");
    let mut child = c
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no vault is configured"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
