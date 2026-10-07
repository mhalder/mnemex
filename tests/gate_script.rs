//! The gate script's own guards and steps.
//!
//! The script runs against stub `cargo`, `rustup`, `npm`, `npx`, and `node`
//! commands that only log their arguments, so no gate actually runs from inside
//! the test suite.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

/// Run the gate with `COVERAGE_FLOOR=floor` and no stage, returning its output
/// and the stubbed commands it ran, one per line, each prefixed with its name.
fn run(floor: &str) -> (Output, String) {
    run_args(floor, None, &[], &[])
}

/// [`run`], with one stage argument.
fn run_stage(stage: &str) -> (Output, String) {
    run_args("", Some(stage), &[], &[])
}

/// [`run_stage`], without the stubs named in `missing`.
fn run_stage_with(stage: &str, missing: &[&str]) -> (Output, String) {
    run_args("", Some(stage), missing, &[])
}

/// [`run`], without the stubs named in `missing` and with `env` set. The stubs
/// read `NODE_MAJOR` (default 24) for `node -p` and `MSRV_INSTALLED` for
/// `rustup run`, which fails unless it is set.
fn run_with(floor: &str, missing: &[&str], env: &[(&str, &str)]) -> (Output, String) {
    run_args(floor, None, missing, env)
}

/// The full form: a floor, an optional stage argument, stubs to omit, and extra
/// environment.
fn run_args(
    floor: &str,
    stage: Option<&str>,
    missing: &[&str],
    env: &[(&str, &str)],
) -> (Output, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("calls.log");
    for name in [
        "cargo",
        "rustup",
        "cargo-deny",
        "cargo-llvm-cov",
        "npm",
        "npx",
        "node",
    ] {
        if missing.contains(&name) {
            continue;
        }
        let stub = dir.path().join(name);
        std::fs::write(
            &stub,
            // A step that sets RUSTDOCFLAGS is logged with it, on the line
            // before its own.
            format!(
                "#!/bin/sh\n[ -n \"$RUSTDOCFLAGS\" ] && echo \"{name} RUSTDOCFLAGS=$RUSTDOCFLAGS\" >> \"$CALL_LOG\"\necho \"{name} $*\" >> \"$CALL_LOG\"\n{}",
                match name {
                    "node" => "[ \"$1\" = -p ] && echo \"${NODE_MAJOR:-24}\"\nexit 0\n",
                    "rustup" => "[ \"$1\" = run ] && [ -z \"$MSRV_INSTALLED\" ] && exit 1\nexit 0\n",
                    _ => "",
                }
            ),
        )
        .expect("stub");
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    let mut cmd = Command::new("bash");
    cmd.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/gate.sh"));
    if let Some(stage) = stage {
        cmd.arg(stage);
    }
    let out = cmd
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
        .env("COVERAGE_FLOOR", floor)
        .env("CALL_LOG", &log)
        .env_remove("RUSTDOCFLAGS")
        .env_remove("NODE_MAJOR")
        .env_remove("MSRV_INSTALLED")
        .envs(env.iter().copied())
        .output()
        .expect("run the gate");
    let calls = std::fs::read_to_string(&log).unwrap_or_default();
    (out, calls)
}

/// The calls that are gates, not the prerequisite probes before them.
fn gates(calls: &str) -> Vec<&str> {
    calls
        .lines()
        .filter(|l| !l.starts_with("node -p") && !l.starts_with("rustup run"))
        .collect()
}

/// The `rust-version` the manifest declares.
fn rust_version() -> String {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("manifest");
    manifest
        .lines()
        .find_map(|l| l.strip_prefix("rust-version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("a rust-version")
        .to_owned()
}

#[test]
fn a_floor_below_the_default_is_refused_before_any_gate_runs() {
    for floor in ["94", "0"] {
        let (out, calls) = run(floor);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{floor}: {stderr}");
        assert_eq!(calls, "", "{floor}: no gate may run");
        assert!(stderr.contains("COVERAGE_FLOOR"), "{floor}: {stderr}");
    }
}

#[test]
fn a_floor_that_is_not_a_whole_number_is_refused() {
    for floor in ["abc", "95.5", "-1"] {
        let (out, calls) = run(floor);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{floor}: {stderr}");
        assert_eq!(calls, "", "{floor}: no gate may run");
    }
}

#[test]
fn the_default_or_a_higher_floor_reaches_the_coverage_step() {
    for floor in ["", "95", "99"] {
        let (out, calls) = run(floor);
        assert!(out.status.success(), "{floor:?}: {out:?}");
        let expected = if floor.is_empty() { "95" } else { floor };
        let coverage = calls.lines().last().expect("a coverage call");
        assert!(
            coverage.contains(&format!(
                "--fail-under-lines {expected} --fail-under-file-lines {expected}"
            )),
            "{floor:?}: {coverage}"
        );
    }
}

#[test]
fn the_declared_rust_version_is_installed_and_builds_every_target() {
    let (out, calls) = run("");
    assert!(out.status.success(), "{out:?}");
    let msrv = rust_version();
    let lines: Vec<&str> = calls.lines().collect();
    let install = lines
        .iter()
        .position(|l| l.starts_with(&format!("rustup toolchain install {msrv} ")))
        .unwrap_or_else(|| panic!("no install of {msrv}:\n{calls}"));
    let check = lines
        .iter()
        .position(|l| l.starts_with(&format!("cargo +{msrv} check --all-targets")))
        .unwrap_or_else(|| panic!("no check on {msrv}:\n{calls}"));
    assert!(install < check, "{calls}");
}

/// A dangling doc link is a warning rustdoc prints and nothing else fails on,
/// so the docs are built with warnings denied, right after clippy.
#[test]
fn the_docs_build_with_warnings_denied() {
    let (out, calls) = run("");
    assert!(out.status.success(), "{out:?}");
    let lines: Vec<&str> = calls.lines().collect();
    let at = |prefix: &str| {
        lines
            .iter()
            .position(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no `{prefix}`:\n{calls}"))
    };
    let clippy = at("cargo clippy ");
    let flags = at("cargo RUSTDOCFLAGS=-D warnings");
    let doc = at("cargo doc --no-deps");
    let msrv = at(&format!("cargo +{} check", rust_version()));
    assert!(clippy < flags && flags + 1 == doc && doc < msrv, "{calls}");
}

#[test]
fn the_pi_extension_is_type_checked_and_tested_from_a_clean_install() {
    let (out, calls) = run("");
    assert!(out.status.success(), "{out:?}");
    let lines: Vec<&str> = calls.lines().collect();
    let at = |prefix: &str| {
        lines
            .iter()
            .position(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no `{prefix}`:\n{calls}"))
    };
    let install = at("npm ci ");
    let typecheck = at("npx --no-install tsc -p tsconfig.json");
    let test = at("node --test tests/pi/");
    assert!(install < typecheck && typecheck < test, "{calls}");
}

#[test]
fn a_missing_tool_is_refused_before_any_gate_runs() {
    for tool in ["cargo-deny", "cargo-llvm-cov", "node"] {
        let (out, calls) = run_with("", &[tool], &[]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{tool}: {stderr}");
        assert!(
            gates(&calls).is_empty(),
            "{tool}: no gate may run:\n{calls}"
        );
        assert!(stderr.contains(tool), "{tool}: {stderr}");
    }
}

#[test]
fn a_node_too_old_for_the_hooks_is_refused_before_any_gate_runs() {
    let (out, calls) = run_with("", &[], &[("NODE_MAJOR", "22")]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(gates(&calls).is_empty(), "no gate may run:\n{calls}");
    assert!(stderr.contains("found 22"), "{stderr}");
}

#[test]
fn an_installed_rust_version_is_not_installed_again() {
    let (out, calls) = run_with("", &[], &[("MSRV_INSTALLED", "1")]);
    assert!(out.status.success(), "{out:?}");
    assert!(!calls.contains("rustup toolchain install"), "{calls}");
    assert!(
        calls.contains(&format!("cargo +{} check", rust_version())),
        "{calls}"
    );
}

/// Without `--locked`, a manifest change the lockfile does not match is
/// resolved quietly mid-gate instead of failing it.
#[test]
fn every_cargo_build_is_locked() {
    let (out, calls) = run("");
    assert!(out.status.success(), "{out:?}");
    let builds: Vec<&str> = calls
        .lines()
        .filter(|l| {
            // The subcommand, past a `+toolchain`.
            let subcommand = l
                .strip_prefix("cargo ")
                .and_then(|rest| rest.split_whitespace().find(|w| !w.starts_with('+')));
            subcommand.is_some_and(|s| ["clippy", "doc", "check", "test", "llvm-cov"].contains(&s))
        })
        .collect();
    assert_eq!(builds.len(), 5, "{calls}");
    for build in builds {
        assert!(build.contains("--locked"), "{build}");
    }
}

#[test]
fn advisories_are_checked_once_by_cargo_deny() {
    let (out, calls) = run("");
    assert!(out.status.success(), "{out:?}");
    assert!(calls.lines().any(|l| l == "cargo deny check"), "{calls}");
    assert!(!calls.contains("cargo audit"), "{calls}");
}

// --- the stage argument ------------------------------------------------------

#[test]
fn all_runs_the_stages_in_order() {
    let (out, calls) = run("");
    assert!(out.status.success(), "{out:?}");
    let gates = gates(&calls);
    let at = |prefix: &str| {
        gates
            .iter()
            .position(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no `{prefix}`:\n{calls}"))
    };
    let fmt = at("cargo fmt ");
    let clippy = at("cargo clippy ");
    let doc = at("cargo doc ");
    let msrv = at(&format!("cargo +{} check", rust_version()));
    let test = at("cargo test");
    let node = at("npm ci ");
    let deny = at("cargo deny check");
    let coverage = at("cargo llvm-cov ");
    assert!(
        fmt < clippy
            && clippy < doc
            && doc < msrv
            && msrv < test
            && test < node
            && node < deny
            && deny < coverage,
        "{calls}"
    );
}

#[test]
fn no_argument_is_identical_to_all() {
    let (bare, bare_calls) = run("");
    let (all, all_calls) = run_stage("all");
    assert!(bare.status.success(), "{bare:?}");
    assert_eq!(all.status.code(), bare.status.code());
    assert_eq!(all_calls, bare_calls);
}

/// A single stage checks only its own prerequisites: `fmt` needs cargo alone,
/// so the node trio and cargo-deny being absent must not stop it.
#[test]
fn fmt_runs_alone_and_ignores_other_stages_prerequisites() {
    let (out, calls) = run_stage_with("fmt", &["cargo-deny", "node", "npx", "npm"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        ["cargo fmt --all -- --check"],
        "{calls}"
    );
}

#[test]
fn deny_with_cargo_deny_missing_exits_two_and_runs_no_gate() {
    let (out, calls) = run_stage_with("deny", &["cargo-deny"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert_eq!(calls, "", "no gate may run:\n{calls}");
    assert!(stderr.contains("cargo-deny"), "{stderr}");
}

#[test]
fn deny_with_cargo_missing_exits_two_and_runs_no_gate() {
    let (out, calls) = run_stage_with("deny", &["cargo"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert_eq!(calls, "", "no gate may run:\n{calls}");
    assert!(stderr.contains("cargo"), "{stderr}");
}

#[test]
fn an_unknown_stage_exits_two_with_usage() {
    let (out, calls) = run_stage("bogus");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert_eq!(calls, "", "no gate may run:\n{calls}");
    assert!(stderr.contains("bogus"), "{stderr}");
    for stage in [
        "fmt", "clippy", "doc", "msrv", "test", "node", "deny", "coverage", "all",
    ] {
        assert!(stderr.contains(stage), "usage names `{stage}`: {stderr}");
    }
}

/// The floor guard sits above the stage dispatch, so it fires for a single
/// stage too.
#[test]
fn the_floor_guard_fires_for_a_single_stage() {
    let (out, calls) = run_args("94", Some("fmt"), &[], &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert_eq!(calls, "", "no gate may run:\n{calls}");
    assert!(stderr.contains("COVERAGE_FLOOR"), "{stderr}");
}
