//! The install/print verbs: `skill`, `extension`, and `completion`.
//!
//! These live in the binary, so they are driven through `mnemex` itself.

use std::process::Command;

use assert_cmd::prelude::*;

fn run(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("mnemex")
        .expect("binary")
        .args(args)
        .env_remove("MEMEX_VAULT")
        .output()
        .expect("run")
}

fn run_home(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("mnemex")
        .expect("binary")
        .args(args)
        .env_remove("MEMEX_VAULT")
        .env("HOME", home)
        .output()
        .expect("run")
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn skill_prints_the_embedded_skill() {
    let out = run(&["skill"]);
    assert!(out.status.success(), "{out:?}");
    let text = stdout(&out);
    assert!(text.starts_with("---\nname: mnemex\n"), "{text}");
    assert!(text.contains("`mnemex schema`"), "{text}");
}

#[test]
fn skill_installs_and_reports_each_outcome() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("skills");

    let out = run(&["skill", "--install", target.to_str().unwrap()]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        stdout(&out),
        format!("created {}/SKILL.md\n", target.display())
    );
    assert_eq!(
        std::fs::read(target.join("SKILL.md")).expect("read"),
        include_bytes!("../skills/mnemex/SKILL.md")
    );

    let out = run(&["skill", "--install", target.to_str().unwrap()]);
    assert_eq!(
        stdout(&out),
        format!("up to date {}/SKILL.md\n", target.display())
    );

    std::fs::write(target.join("SKILL.md"), "stale").expect("write");
    let out = run(&["skill", "--install", target.to_str().unwrap()]);
    assert_eq!(
        stdout(&out),
        format!("updated {}/SKILL.md\n", target.display())
    );
    assert_eq!(
        std::fs::read(target.join("SKILL.md")).expect("read"),
        include_bytes!("../skills/mnemex/SKILL.md")
    );
}

#[test]
fn skill_check_exits_one_when_missing_or_stale() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("nope");

    let out = run(&["skill", "--check", missing.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");

    let target = dir.path().join("skills");
    run(&["skill", "--install", target.to_str().unwrap()]);
    assert_eq!(
        run(&["skill", "--check", target.to_str().unwrap()])
            .status
            .code(),
        Some(0)
    );

    std::fs::write(target.join("SKILL.md"), "stale").expect("write");
    let out = run(&["skill", "--check", target.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

#[test]
fn check_against_a_directory_exits_two_not_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("skills");
    std::fs::create_dir_all(target.join("SKILL.md")).expect("mkdir");

    let out = run(&["skill", "--check", target.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("missing or out of date"),
        "{out:?}"
    );
}

#[test]
fn extension_prints_and_installs_a_hook() {
    let out = run(&["extension", "pi"]);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("export default function mnemexHooks"));

    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("ext");
    let out = run(&["extension", "pi", "--install", target.to_str().unwrap()]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        stdout(&out),
        format!("created {}/mnemex.ts\n", target.display())
    );
    assert_eq!(
        std::fs::read(target.join("mnemex.ts")).expect("read"),
        include_bytes!("../extensions/pi.ts")
    );

    let out = run(&["extension", "pi", "--check", target.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        stdout(&out),
        format!("up to date {}/mnemex.ts\n", target.display())
    );
}

#[test]
fn extension_claude_install_prints_the_registration_snippet() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("hooks");
    let out = run(&["extension", "claude", "--install", target.to_str().unwrap()]);
    assert!(out.status.success(), "{out:?}");
    let text = stdout(&out);
    assert!(
        text.contains("register in ~/.claude/settings.json"),
        "{text}"
    );
    assert!(
        text.contains(&format!("node {}/mnemex.ts", target.display())),
        "{text}"
    );
    assert!(text.contains("\"matcher\": \"Write|Edit\""), "{text}");
}

#[test]
fn skill_defaults_to_the_home_skill_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = run_home(dir.path(), &["skill", "--install"]);
    assert!(out.status.success(), "{out:?}");
    let path = dir.path().join(".agents/skills/mnemex/SKILL.md");
    assert_eq!(stdout(&out), format!("created {}\n", path.display()));
    assert!(path.is_file());
}

#[test]
fn extension_pi_defaults_to_the_agent_extensions_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let agent = dir.path().join("agent");
    let out = run_home(dir.path(), &["extension", "pi", "--install"]);
    assert!(out.status.success(), "{out:?}");
    let path = dir.path().join(".pi/agent/extensions/mnemex.ts");
    assert_eq!(stdout(&out), format!("created {}\n", path.display()));
    assert!(path.is_file());

    // PI_CODING_AGENT_DIR overrides the agent directory.
    let out = Command::cargo_bin("mnemex")
        .expect("binary")
        .args(["extension", "pi", "--install"])
        .env_remove("MEMEX_VAULT")
        .env("HOME", dir.path())
        .env("PI_CODING_AGENT_DIR", &agent)
        .output()
        .expect("run");
    assert!(out.status.success(), "{out:?}");
    let path = agent.join("extensions/mnemex.ts");
    assert_eq!(stdout(&out), format!("created {}\n", path.display()));
    assert!(path.is_file());
}

#[test]
fn extension_claude_defaults_to_the_home_hooks_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = run_home(dir.path(), &["extension", "claude", "--install"]);
    assert!(out.status.success(), "{out:?}");
    let path = dir.path().join(".claude/hooks/mnemex.ts");
    assert!(stdout(&out).contains(&format!("created {}\n", path.display())));
    assert!(path.is_file());
}

#[test]
fn completion_prints_each_shells_activation_snippet() {
    let cases = [
        ("bash", "source <(COMPLETE=bash mnemex)\n"),
        ("zsh", "source <(COMPLETE=zsh mnemex)\n"),
        ("fish", "COMPLETE=fish mnemex | source\n"),
        ("elvish", "eval (E:COMPLETE=elvish mnemex | slurp)\n"),
        (
            "powershell",
            "$env:COMPLETE = \"powershell\"; mnemex | Out-String | Invoke-Expression; Remove-Item Env:\\COMPLETE\n",
        ),
    ];
    for (shell, want) in cases {
        let out = run(&["completion", shell]);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(stdout(&out), want, "{shell}");
    }
}

#[test]
fn install_and_check_are_mutually_exclusive() {
    let out = run(&["skill", "--install", "--check"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}
