//! Shell completion: ids by any part of their id or title, and the schema's
//! closed sets.

mod support;

use std::process::Command;

use assert_cmd::prelude::*;
use clap_complete::engine::CompletionCandidate;
use mnemex::complete;
use mnemex::kind::Kind;
use support::Vault;

fn seed(v: &Vault) {
    v.write(
        "projects",
        "202609231653-home-lab",
        "---\nstatus: active\ntags:\n  - infra\n---\n\n# Home Lab\n",
    );
    v.write(
        "projects",
        "202609221702-mnemex",
        "---\nstatus: active\n---\n\n# mnemex\n",
    );
    v.write(
        "plans",
        "202609250900-harden-home",
        "---\nproject: \"[[202609231653-home-lab]]\"\nstatus: open\ntags:\n  - infra\n  - security\n---\n\n# Harden the box\n",
    );
}

fn values(candidates: &[CompletionCandidate]) -> Vec<String> {
    candidates
        .iter()
        .map(|c| c.get_value().to_string_lossy().into_owned())
        .collect()
}

fn helps(candidates: &[CompletionCandidate]) -> Vec<String> {
    candidates
        .iter()
        .map(|c| c.get_help().map(ToString::to_string).unwrap_or_default())
        .collect()
}

fn words(ws: &[&str]) -> Vec<String> {
    ws.iter().map(ToString::to_string).collect()
}

#[test]
fn a_word_inside_the_id_completes_to_the_whole_id() {
    let v = Vault::new();
    seed(&v);
    let found = complete::notes(v.root(), Some(Kind::Project), "lab");
    assert_eq!(values(&found), ["202609231653-home-lab"]);
    assert_eq!(helps(&found), ["Home Lab"]);
}

#[test]
fn every_kind_is_offered_with_its_kind_in_the_help() {
    let v = Vault::new();
    seed(&v);
    let found = complete::notes(v.root(), None, "home");
    assert_eq!(
        values(&found),
        ["202609231653-home-lab", "202609250900-harden-home"]
    );
    assert_eq!(helps(&found), ["project: Home Lab", "plan: Harden the box"]);
}

#[test]
fn the_title_matches_too_regardless_of_case() {
    let v = Vault::new();
    seed(&v);
    let found = complete::notes(v.root(), None, "BOX");
    assert_eq!(values(&found), ["202609250900-harden-home"]);
}

#[test]
fn a_missing_vault_offers_nothing() {
    let v = Vault::new();
    let gone = v.root().join("nowhere");
    assert!(complete::notes(&gone, None, "").is_empty());
    assert!(complete::tags(&gone, "").is_empty());
}

#[test]
fn tags_in_use_are_offered_once_each() {
    let v = Vault::new();
    seed(&v);
    assert_eq!(values(&complete::tags(v.root(), "")), ["infra", "security"]);
    assert_eq!(values(&complete::tags(v.root(), "cur")), ["security"]);
}

#[test]
fn kinds_come_from_the_kind_table() {
    assert_eq!(
        values(&complete::kinds()),
        ["project", "memory", "plan", "adr", "context"]
    );
}

#[test]
fn statuses_come_from_the_schema_with_the_kinds_that_take_them() {
    let found = complete::statuses();
    let pairs: Vec<_> = values(&found).into_iter().zip(helps(&found)).collect();
    assert!(pairs.contains(&("active".into(), "project".into())));
    assert!(pairs.contains(&("open".into(), "plan".into())));
    assert!(pairs.contains(&("proposed".into(), "adr".into())));
}

#[test]
fn a_field_several_kinds_share_is_offered_once() {
    let found = complete::fields();
    let tags: Vec<_> = helps(&found)
        .into_iter()
        .zip(values(&found))
        .filter(|(_, v)| v == "tags")
        .collect();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].0, "project, memory, plan, adr, context");
}

#[test]
fn set_offers_the_named_notes_fields_or_every_field() {
    let v = Vault::new();
    seed(&v);
    let plan = complete::set_field(v.root(), &words(&["202609250900-harden-home"]), "");
    assert_eq!(values(&plan), ["project", "status", "tags", "refs"]);
    let unknown = complete::set_field(v.root(), &words(&["nope"]), "pa");
    assert_eq!(values(&unknown), ["path"]);
}

#[test]
fn set_offers_values_by_the_fields_shape() {
    let v = Vault::new();
    seed(&v);
    let plan = "202609250900-harden-home";
    let status = complete::set_value(v.root(), &words(&[plan, "status"]), "");
    assert_eq!(values(&status), ["open", "done"]);
    let project = complete::set_value(v.root(), &words(&[plan, "project"]), "home");
    assert_eq!(values(&project), ["202609231653-home-lab"]);
    let tags = complete::set_value(v.root(), &words(&[plan, "tags"]), "in");
    assert_eq!(values(&tags), ["infra"]);
    assert!(complete::set_value(v.root(), &words(&[plan, "refs"]), "").is_empty());
    assert!(complete::set_value(v.root(), &words(&[plan, "nope"]), "").is_empty());
    assert!(complete::set_value(v.root(), &words(&[plan]), "").is_empty());
}

#[test]
fn set_words_are_the_positionals_before_the_cursor() {
    let args = words(&[
        "mnemex", "--", "mnemex", "set", "--clear", "id", "status", "op",
    ]);
    assert_eq!(complete::set_words(&args), ["id", "status"]);
    assert!(complete::set_words(&words(&["mnemex", "--", "mnemex", "show", "x"])).is_empty());
}

/// The binary answers a completion request in each shell, so the entry points
/// `cli` attaches are the ones the shell reaches. Fish derives the cursor from
/// the word list; bash and zsh read it from `_CLAP_COMPLETE_INDEX`, which
/// their registration scripts set.
#[test]
fn the_binary_answers_each_shell() {
    let v = Vault::new();
    seed(&v);
    let ask = |shell: &str, line: &[&str]| {
        let mut cmd = Command::cargo_bin("mnemex").expect("binary");
        cmd.env("MNEMEX_VAULT", v.root()).env("COMPLETE", shell);
        if shell != "fish" {
            // The last word is the one under the cursor.
            cmd.env("_CLAP_COMPLETE_INDEX", (line.len() - 1).to_string());
        }
        let out = cmd.arg("--").args(line).output().expect("run");
        assert!(out.status.success(), "{shell} {line:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    // Candidates without help: bash and zsh print just the value, fish
    // appends a newline.
    assert_eq!(
        ask("bash", &["mnemex", "new", "adr", "x", "--tag", "sec"]),
        "security"
    );
    assert_eq!(
        ask("zsh", &["mnemex", "new", "adr", "x", "--tag", "sec"]),
        "security"
    );
    assert_eq!(
        ask("fish", &["mnemex", "new", "adr", "x", "--tag", "sec"]),
        "security\n"
    );
    assert_eq!(
        ask("bash", &["mnemex", "set", "202609250900-harden-home", "st"]),
        "status"
    );
    assert_eq!(
        ask("zsh", &["mnemex", "set", "202609250900-harden-home", "st"]),
        "status"
    );
    assert_eq!(
        ask("fish", &["mnemex", "set", "202609250900-harden-home", "st"]),
        "status\n"
    );
    assert_eq!(
        ask(
            "bash",
            &["mnemex", "set", "202609250900-harden-home", "status", "d"]
        ),
        "done"
    );
    assert_eq!(
        ask(
            "zsh",
            &["mnemex", "set", "202609250900-harden-home", "status", "d"]
        ),
        "done"
    );
    assert_eq!(
        ask(
            "fish",
            &["mnemex", "set", "202609250900-harden-home", "status", "d"]
        ),
        "done\n"
    );

    // Candidates with help print per shell: bash values only, fish
    // `value\thelp`, zsh `value:help`. Only fish ends with a newline.
    assert_eq!(
        ask("bash", &["mnemex", "show", "home"]),
        "202609231653-home-lab\n202609250900-harden-home"
    );
    assert_eq!(
        ask("fish", &["mnemex", "show", "home"]),
        "202609231653-home-lab\tproject: Home Lab\n202609250900-harden-home\tplan: Harden the box\n"
    );
    assert_eq!(
        ask("zsh", &["mnemex", "show", "home"]),
        "202609231653-home-lab:project: Home Lab\n202609250900-harden-home:plan: Harden the box"
    );

    assert_eq!(
        ask("bash", &["mnemex", "list", "--project", "home"]),
        "202609231653-home-lab"
    );
    assert_eq!(
        ask("fish", &["mnemex", "list", "--project", "home"]),
        "202609231653-home-lab\tHome Lab\n"
    );
    assert_eq!(
        ask("zsh", &["mnemex", "list", "--project", "home"]),
        "202609231653-home-lab:Home Lab"
    );
}

/// `mnemex completion <shell>` prints the activation snippet a shell sources
/// on startup; it must parse in that shell. A shell that is not installed
/// skips rather than fails: CI installs zsh and fish, a developer's machine
/// may not have them.
#[test]
fn every_completion_snippet_is_valid_shell_syntax() {
    for (shell, binary, args) in [
        ("bash", "bash", &["-n"][..]),
        ("zsh", "zsh", &["-n"][..]),
        ("fish", "fish", &["--no-execute"][..]),
    ] {
        let out = Command::cargo_bin("mnemex")
            .expect("binary")
            .env_remove("MNEMEX_VAULT")
            .args(["completion", shell])
            .output()
            .expect("run");
        assert!(out.status.success(), "{out:?}");
        let snippet = String::from_utf8_lossy(&out.stdout).into_owned();

        let dir = tempfile::tempdir().expect("tempdir");
        let script = dir.path().join("completion");
        std::fs::write(&script, &snippet).expect("write");
        let checked = match Command::new(binary).args(args).arg(&script).output() {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("skipping {shell} completion syntax check: {binary} is not installed");
                continue;
            }
            Err(e) => panic!("run {binary}: {e}"),
        };
        assert!(
            checked.status.success(),
            "{shell} syntax check failed for:\n{snippet}\n---\n{}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }
}
