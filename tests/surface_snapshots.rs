//! The command surface, snapshot-tested.
//!
//! The skill describes these verbs, so a rename should surface as a snapshot
//! diff rather than as a skill that quietly lies.

mod support;

use std::process::Command;

use assert_cmd::prelude::*;
use support::Vault;

fn help(args: &[&str]) -> String {
    let out = Command::cargo_bin("mnemex")
        .expect("binary")
        .args(args)
        .arg("--help")
        .env_remove("MEMEX_VAULT")
        .output()
        .expect("run");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_command_surface() {
    insta::assert_snapshot!("help", help(&[]));
}

#[test]
fn every_verbs_help() {
    for verb in [
        "new",
        "set",
        "rename",
        "delete",
        "adopt",
        "check",
        "schema",
        "hook",
        "list",
        "resolve",
        "project",
        "show",
        "brief",
        "skill",
        "extension",
        "completion",
    ] {
        insta::assert_snapshot!(format!("help-{verb}"), help(&[verb]));
    }
}

#[test]
fn both_schema_renderings() {
    let out = Command::cargo_bin("mnemex")
        .expect("binary")
        .arg("schema")
        .output()
        .expect("run");
    insta::assert_snapshot!("schema-human", String::from_utf8_lossy(&out.stdout));

    let out = Command::cargo_bin("mnemex")
        .expect("binary")
        .args(["schema", "--json"])
        .output()
        .expect("run");
    insta::assert_snapshot!("schema-json", String::from_utf8_lossy(&out.stdout));
}

/// A vault that trips one rule from every band, so the report exercises spans,
/// spanless findings, both severities and the grouping.
fn messy(v: &Vault) {
    v.note(
        "projects",
        "202609061846-p",
        "status: active\npath: not-here\nrepo: github.com/owner/mnemex\n",
    );
    v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: maybe\nowner: me\n",
    );
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    v.write(
        "plans",
        "notanid",
        "---\nproject: \"[[202609061846-p]]\"\n---\n",
    );
}

fn check_output(v: &Vault, json: bool) -> String {
    let mut c = Command::cargo_bin("mnemex").expect("binary");
    c.args(["check"]);
    if json {
        c.arg("--json");
    }
    let out = c.env("MEMEX_VAULT", v.root()).output().expect("run");
    String::from_utf8_lossy(&out.stdout).replace(&v.root().display().to_string(), "<vault>")
}

#[test]
fn the_human_check_report() {
    let v = Vault::new();
    messy(&v);
    insta::assert_snapshot!("check-human", check_output(&v, false));
}

#[test]
fn the_check_envelope() {
    let v = Vault::new();
    messy(&v);
    insta::assert_snapshot!("check-json", check_output(&v, true));
}
