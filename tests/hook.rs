//! The write hook: what it blocks on, what it informs about, and
//! everything it stays silent for.

mod support;

use mnemex::check::Env;
use mnemex::hook;
use support::Vault;

fn payload(path: &std::path::Path) -> String {
    serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Write",
        "tool_input": { "file_path": path },
    })
    .to_string()
}

fn project(v: &Vault, id: &str) {
    v.note("projects", id, "status: active\n");
}

#[test]
fn a_clean_write_is_silent() {
    let v = Vault::new();
    let p = v.note(
        "memories",
        "202609081100-n",
        "project: \"[[202609061846-p]]\"\n",
    );
    project(&v, "202609061846-p");
    assert_eq!(
        hook::respond_at(&payload(&p), v.root(), Env::default()).0,
        None
    );
}

#[test]
fn a_write_outside_a_governed_folder_is_silent() {
    let v = Vault::new();
    let p = v.write("daily", "2026-09-08", "not even frontmatter\n");
    assert_eq!(
        hook::respond_at(&payload(&p), v.root(), Env::default()).0,
        None
    );
}

#[test]
fn errors_block_the_write_with_the_human_report() {
    let v = Vault::new();
    let p = v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    project(&v, "202609061846-p");
    let out = hook::respond_at(&payload(&p), v.root(), Env::default())
        .0
        .expect("blocked");
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(v["decision"], "block");
    let reason = v["reason"].as_str().expect("reason");
    assert!(reason.starts_with(
        "This post-write check found the vault inconsistent. Fix it with the `mnemex` verbs, or by editing the body:\n\n"
    ), "{reason}");
    assert!(reason.contains("error[MX102]"), "{reason}");
}

#[test]
fn warnings_alone_inform_rather_than_block() {
    let v = Vault::new();
    project(&v, "202609061846-p");
    let p = v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\nowner: me\n",
    );

    let out = hook::respond_at(&payload(&p), v.root(), Env::default())
        .0
        .expect("informed");
    let j: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(j["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    let ctx = j["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("context");
    assert!(ctx.contains("warning[MX105]"), "{ctx}");
    assert!(j.get("decision").is_none(), "a warning must not block");
}

#[test]
fn a_pi_tool_result_payload_names_the_path_as_input_path() {
    let v = Vault::new();
    let p = v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    project(&v, "202609061846-p");
    let payload = serde_json::json!({ "toolName": "write", "input": { "path": p } }).to_string();
    let out = hook::respond_at(&payload, v.root(), Env::default())
        .0
        .expect("blocked");
    assert!(out.contains("MX102"), "{out}");
}

#[test]
fn the_path_may_come_from_either_response_shape() {
    let v = Vault::new();
    let p = v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    project(&v, "202609061846-p");
    for key in ["file_path", "filePath"] {
        let payload = serde_json::json!({ "tool_response": { key: p } }).to_string();
        let out = hook::respond_at(&payload, v.root(), Env::default())
            .0
            .expect("blocked");
        assert!(out.contains("\"block\""), "{key}: {out}");
    }
}

#[test]
fn a_sibling_of_an_unexpected_type_does_not_hide_the_written_path() {
    let v = Vault::new();
    let p = v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    project(&v, "202609061846-p");
    for payload in [
        serde_json::json!({ "tool_input": { "file_path": p }, "tool_response": "File written" }),
        serde_json::json!({ "tool_input": { "path": 3, "file_path": p } }),
        serde_json::json!({ "tool_input": { "file_path": p }, "input": [1, 2] }),
    ] {
        let out = hook::respond_at(&payload.to_string(), v.root(), Env::default());
        assert!(
            out.0.as_deref().is_some_and(|o| o.contains("MX102")),
            "{payload}: {:?}",
            out.0
        );
    }
}

#[test]
fn tool_input_wins_over_tool_response() {
    let v = Vault::new();
    let bad = v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    project(&v, "202609061846-p");
    let good = v.note(
        "memories",
        "202609081100-n",
        "project: \"[[202609061846-p]]\"\n",
    );
    let payload = serde_json::json!({
        "tool_input": { "file_path": good },
        "tool_response": { "file_path": bad },
    })
    .to_string();
    assert_eq!(hook::respond_at(&payload, v.root(), Env::default()).0, None);
}

#[test]
fn an_unparseable_payload_is_silent() {
    let v = Vault::new();
    for p in [
        "",
        "not json",
        "{}",
        "[]",
        "{\"tool_input\": {}}",
        "{\"tool_input\": 3}",
    ] {
        assert_eq!(
            hook::respond_at(p, v.root(), Env::default()).0,
            None,
            "{p:?}"
        );
    }
}

#[test]
fn debug_notes_explain_why_the_hook_stayed_silent() {
    let v = Vault::new();
    let (out, debug) = hook::respond_at("not json", v.root(), Env::default());
    assert_eq!(out, None);
    assert_eq!(debug, ["payload is not recognized JSON"]);

    let p = v.write("daily", "2026-09-08", "not even frontmatter\n");
    let (out, debug) = hook::respond_at(&payload(&p), v.root(), Env::default());
    assert_eq!(out, None);
    assert_eq!(
        debug,
        [format!(
            "{} is not a note of the vault at {}",
            p.display(),
            v.root().display()
        )]
    );
}

#[test]
fn a_note_shaped_file_of_another_vault_is_silent() {
    let v = Vault::new();
    let other = Vault::new();
    let p = other.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\n",
    );
    assert_eq!(
        hook::respond_at(&payload(&p), v.root(), Env::default()).0,
        None
    );
    // A folder spelled with `..` is still the vault's own.
    let dotted = v.root().join("adrs/../adrs/202609061400-b.md");
    std::fs::write(
        &dotted,
        "---\nproject: \"[[202609061846-p]]\"\nstatus: maybe\n---\n",
    )
    .expect("write");
    project(&v, "202609061846-p");
    assert!(
        hook::respond_at(&payload(&dotted), v.root(), Env::default())
            .0
            .is_some_and(|out| out.contains("MX102"))
    );
}

#[test]
fn an_unreadable_file_is_silent() {
    let v = Vault::new();
    let missing = v.root().join("plans/202609081100-gone.md");
    assert_eq!(
        hook::respond_at(&payload(&missing), v.root(), Env::default()).0,
        None
    );
}

#[test]
fn a_vault_the_check_cannot_read_is_silent_with_a_debug_note() {
    let v = Vault::new();
    let p = v.note(
        "memories",
        "202609081100-n",
        "project: \"[[202609061846-p]]\"\n",
    );
    project(&v, "202609061846-p");
    // A governed folder name held by a file cannot be listed, so the check
    // fails on I/O rather than reporting on the note.
    std::fs::remove_file(v.root().join("projects/202609061846-p.md")).expect("rm");
    std::fs::remove_dir(v.root().join("projects")).expect("rmdir");
    std::fs::write(v.root().join("projects"), "").expect("write");

    let (out, debug) = hook::respond_at(&payload(&p), v.root(), Env::default());
    assert_eq!(out, None);
    let prefix = format!("could not check {}: ", p.display());
    assert!(
        debug.len() == 1 && debug[0].starts_with(&prefix),
        "{debug:?}"
    );
}

#[test]
fn the_hook_catches_a_dangling_link_in_the_written_note() {
    let v = Vault::new();
    let p = v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609079999-gone]]\"\nstatus: open\n",
    );
    let out = hook::respond_at(&payload(&p), v.root(), Env::default())
        .0
        .expect("blocked");
    assert!(out.contains("MX202"), "{out}");
}
