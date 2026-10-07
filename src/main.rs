//! The `mnemex` binary: argument parsing, printing, and exit codes.
//!
//! Every decision lives in the library; this decides only what to print and
//! what to exit with: 0 clean, 1 errors present (`check`) or a query that found
//! nothing or could not answer fully, 2 usage error, I/O failure, or a refused
//! verb.

use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory as _, Parser as _};
use mnemex::authoring::{self, Ctx, NewArgs};
use mnemex::brief;
use mnemex::check::{self, Env};
use mnemex::cli::{Cli, Command, CompletionShell, ExtensionTarget};
use mnemex::error::Error;
use mnemex::id;
use mnemex::kind::Kind;
use mnemex::query::{self, ProjectAnswer};
use mnemex::report::{self, Summary};
use mnemex::{hook, schema, vault};

/// The embedded agent skill.
const SKILL: &str = include_str!("../skills/mnemex/SKILL.md");
/// The embedded Pi extension hook.
const PI_HOOK: &str = include_str!("../extensions/pi.ts");
/// The embedded Claude Code command hook.
const CLAUDE_HOOK: &str = include_str!("../hooks/claude.ts");
/// The filename the skill is installed as.
const SKILL_FILE: &str = "SKILL.md";
/// The filename the Pi extension and the Claude hook are installed as.
const EXTENSION_FILE: &str = "mnemex.ts";

/// Clean.
const OK: u8 = 0;
/// Errors present, or a query that found nothing.
const FOUND_PROBLEMS: u8 = 1;
/// Usage error, I/O failure, or a refused verb.
const REFUSED: u8 = 2;

fn main() -> ExitCode {
    // Answers a shell's completion request and exits when `COMPLETE` is set;
    // otherwise does nothing.
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    match run(&cli.command) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "error: {e}");
            ExitCode::from(REFUSED)
        }
    }
}

/// Write to stdout, treating a closed pipe (`mnemex check | head`) as a
/// normal end rather than a panic.
fn print(text: &str) {
    let _ = std::io::stdout().write_all(text.as_bytes());
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn cwd() -> Result<PathBuf, Error> {
    std::env::current_dir().map_err(|e| Error::io(".", e))
}

/// A project directory as the person typing it means it.
///
/// The library resolves a relative `path` against the vault root, which is what
/// lets a fixture vault carry `path: work`. Someone typing `--path .` in a
/// checkout means the checkout, so a relative spelling is made absolute against
/// the working directory before the library sees it. `~/` and `$HOME/` stay as
/// written, so they stay portable, and a blank value stays blank.
fn from_cwd(path: &str) -> Result<String, Error> {
    if path.trim().is_empty()
        || path.starts_with("~/")
        || path.starts_with("$HOME/")
        || Path::new(path).is_absolute()
    {
        return Ok(path.to_owned());
    }
    let mut dir = cwd()?;
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                dir.pop();
            }
            other => dir.push(other),
        }
    }
    Ok(dir.display().to_string())
}

/// The values `set` passes on: a `path` taken from the working directory,
/// anything else as given.
fn set_values(field: &str, values: &[String]) -> Result<Vec<String>, Error> {
    if field == "path" {
        values.iter().map(|v| from_cwd(v)).collect()
    } else {
        Ok(values.to_vec())
    }
}

/// The one vault root, refusing when it does not exist.
///
/// Verbs that read fail rather than invent a directory; `new` and `adopt`
/// create it.
fn existing_vault() -> Result<PathBuf, Error> {
    let root = vault::root()?;
    if !root.is_dir() {
        return Err(Error::NoVaultAt { path: root });
    }
    Ok(root)
}

/// The `YYYYMMDDHHMM` local-time stamp a new id starts with.
fn clock() -> String {
    jiff::Zoned::now().strftime("%Y%m%d%H%M").to_string()
}

fn run(command: &Command) -> Result<u8, Error> {
    match command {
        Command::Schema { json } => {
            print(&if *json {
                schema::json()
            } else {
                schema::human()
            });
            Ok(OK)
        }
        Command::Hook => Ok(hook_command()),
        Command::Check { path, json } => check_command(path.as_deref(), *json),
        Command::Resolve { id, json } => Ok(resolve_command(&existing_vault()?, id, *json)),
        Command::List {
            kind,
            project,
            json,
        } => list_command(
            &existing_vault()?,
            kind.as_deref(),
            project.as_deref(),
            *json,
        ),
        Command::Project { for_, json } => {
            let dir = for_.clone().unwrap_or(cwd()?);
            project_command(&existing_vault()?, &dir, *json)
        }
        Command::Show { id, json } => show_command(&existing_vault()?, id, *json),
        Command::Brief { for_, budget, json } => {
            let dir = for_.clone().unwrap_or(cwd()?);
            brief_command(
                &existing_vault()?,
                &dir,
                budget.unwrap_or(brief::DEFAULT_BUDGET),
                *json,
            )
        }
        Command::Skill { install, check } => {
            let mode = match (install, check) {
                (Some(dir), None) => InstallMode::Install(dir.clone()),
                (None, Some(dir)) => InstallMode::Check(dir.clone()),
                (None, None) => InstallMode::Print,
                (Some(_), Some(_)) => {
                    unreachable!("`--install` and `--check` are mutually exclusive")
                }
            };
            skill_command(mode)
        }
        Command::Extension {
            target,
            install,
            check,
        } => {
            let mode = match (install, check) {
                (Some(dir), None) => InstallMode::Install(dir.clone()),
                (None, Some(dir)) => InstallMode::Check(dir.clone()),
                (None, None) => InstallMode::Print,
                (Some(_), Some(_)) => {
                    unreachable!("`--install` and `--check` are mutually exclusive")
                }
            };
            extension_command(*target, mode)
        }
        Command::Completion { shell } => {
            print(completion_snippet(*shell));
            print("\n");
            Ok(OK)
        }
        _ => authoring_command(command),
    }
}

/// The verbs that write. They exit 0 even when the note they wrote still trips
/// a rule: the verb succeeded, and a deliberately blank required field is
/// guidance, not failure.
fn authoring_command(command: &Command) -> Result<u8, Error> {
    let (Command::New { .. }
    | Command::Set { .. }
    | Command::Rename { .. }
    | Command::Delete { .. }
    | Command::Adopt { .. }) = command
    else {
        return Err(Error::NoVault);
    };
    let root = vault::root()?;
    let stamp = clock();
    let home = home();
    let ctx = Ctx {
        root: &root,
        home: home.as_deref(),
        stamp: &stamp,
    };

    print(&report::outcome_human(&outcome_of(ctx, command)?));
    Ok(OK)
}

/// Run one authoring verb in `ctx`.
fn outcome_of(ctx: Ctx<'_>, command: &Command) -> Result<authoring::Outcome, Error> {
    match command {
        Command::New {
            kind,
            title,
            project,
            status,
            tags,
            refs,
            path,
            repo,
        } => authoring::new(
            ctx,
            parse_kind(kind)?,
            title,
            NewArgs {
                project: project.as_deref(),
                status: status.as_deref(),
                tags,
                refs,
                path: path.as_deref().map(from_cwd).transpose()?.as_deref(),
                repo: repo.as_deref(),
            },
        ),
        Command::Set {
            note,
            field,
            values,
            clear,
        } => authoring::set(ctx, note, field, &set_values(field, values)?, *clear),
        Command::Rename {
            note,
            title,
            dry_run,
        } => {
            if *dry_run {
                authoring::rename_preview(ctx, note, title)
            } else {
                authoring::rename(ctx, note, title)
            }
        }
        Command::Delete { note } => authoring::delete(ctx, note),
        Command::Adopt {
            file,
            kind,
            project,
            status,
            tags,
            refs,
        } => authoring::adopt(
            ctx,
            parse_kind(kind)?,
            file,
            NewArgs {
                project: project.as_deref(),
                status: status.as_deref(),
                tags,
                refs,
                path: None,
                repo: None,
            },
        ),
        // `authoring_command` hands over only the authoring variants.
        _ => Err(Error::NoVault),
    }
}

/// The hook always exits 0: it must never fail the write it reports on.
fn hook_command() -> u8 {
    let mut payload = String::new();
    let home = home();
    if std::io::stdin().read_to_string(&mut payload).is_ok() {
        let env = Env {
            home: home.as_deref(),
        };
        if std::env::var_os("MNEMEX_HOOK_DEBUG").is_some() {
            let (out, debug) = hook::respond_with_debug(&payload, env);
            for note in debug {
                eprintln!("mnemex hook: {note}");
            }
            if let Some(out) = out {
                print(&out);
                print("\n");
            }
        } else if let Some(out) = hook::respond(&payload, env) {
            print(&out);
            print("\n");
        }
    }
    OK
}

/// A query that finds nothing exits 1: the question was asked and answered.
fn nothing_found(root: &Path, id: &str) -> u8 {
    let _ = writeln!(std::io::stderr(), "`{id}` is no note in {}", root.display());
    FOUND_PROBLEMS
}

fn resolve_command(root: &Path, id: &str, json: bool) -> u8 {
    let found = query::resolve(root, id);
    if json {
        // The JSON form still emits its envelope, absence included.
        print(&report::resolve_json(found.as_ref()));
    } else if let Some(r) = &found {
        print(&report::resolve_human(r));
    }
    if found.is_some() {
        OK
    } else {
        nothing_found(root, id)
    }
}

/// A note that exists but cannot be shown exits 1, like a query that found
/// nothing: the question was asked, and the answer is incomplete.
fn show_command(root: &Path, id: &str, json: bool) -> Result<u8, Error> {
    let found = query::show(root, id);
    if let Err(e @ Error::NoteDoesNotParse { .. }) = &found {
        let _ = writeln!(std::io::stderr(), "{e}");
        return Ok(FOUND_PROBLEMS);
    }
    let Some(answer) = found? else {
        if json {
            print(&report::resolve_json(None));
        }
        return Ok(nothing_found(root, id));
    };
    print(&if json {
        report::show_json(&answer)
    } else {
        report::show_human(&answer)
    });
    Ok(OK)
}

/// A query that finds nothing exits 1. The message names the filter, because a
/// filtered query that matched nothing must not read as an empty vault.
fn list_command(
    root: &Path,
    kind: Option<&str>,
    project: Option<&str>,
    json: bool,
) -> Result<u8, Error> {
    let kind = kind.map(parse_kind).transpose()?;
    let notes = query::list(root, kind, project)?;
    print(&if json {
        report::list_json(&notes)
    } else {
        report::list_human(root, &notes)
    });
    if notes.is_empty() {
        let filter = match (kind, project) {
            (Some(kind), Some(project)) => {
                format!(" of kind `{}` for project `{project}`", kind.name())
            }
            (Some(kind), None) => format!(" of kind `{}`", kind.name()),
            (None, Some(project)) => format!(" for project `{project}`"),
            (None, None) => String::new(),
        };
        let _ = writeln!(std::io::stderr(), "no notes{filter} in {}", root.display());
        if let Some(project) = project
            && let Some(full) = project_slug_hint(root, project)
        {
            let _ = writeln!(
                std::io::stderr(),
                "`{project}` is a slug, not an id; use `{full}`"
            );
        }
        return Ok(FOUND_PROBLEMS);
    }
    Ok(OK)
}

/// The id of the project whose slug is `value`, when `value` is a slug rather
/// than an id. `--project` takes an id, and the slug is the half a human
/// remembers.
fn project_slug_hint(root: &Path, value: &str) -> Option<String> {
    if id::is_valid(value) {
        return None;
    }
    query::list(root, Some(Kind::Project), None)
        .ok()?
        .into_iter()
        .find(|n| id::slug(&n.id) == Some(value))
        .map(|n| n.id)
}

fn parse_kind(name: &str) -> Result<Kind, Error> {
    Kind::ALL
        .into_iter()
        .find(|k| k.name() == name)
        .ok_or_else(|| Error::UnknownKind {
            found: name.to_owned(),
            kinds: Kind::ALL.iter().map(|k| k.name()).collect(),
        })
}

fn check_command(path: Option<&Path>, json: bool) -> Result<u8, Error> {
    let home = home();
    let env = Env {
        home: home.as_deref(),
    };
    let root = existing_vault()?;
    let (diagnostics, notes) = if let Some(note) = path {
        (check::path(note, &root, env)?, 1)
    } else {
        let files = vault::governed_files(&root)?;
        let notes = files.len();
        (check::root(&root, env)?, notes)
    };
    print(&if json {
        report::check_json(&diagnostics, notes)
    } else {
        report::check_human(&diagnostics, notes)
    });
    Ok(if Summary::of(&diagnostics).has_errors() {
        FOUND_PROBLEMS
    } else {
        OK
    })
}

fn project_command(root: &Path, dir: &Path, json: bool) -> Result<u8, Error> {
    let answer = query::project(root, dir, home().as_deref())?;
    Ok(project_output(&answer, json))
}

fn project_output(answer: &ProjectAnswer, json: bool) -> u8 {
    if json {
        print(&report::project_json(answer));
    } else if answer.note.is_some() {
        print(&report::project_human(answer));
    }
    match (&answer.note, answer.candidates.is_empty()) {
        (Some(_), _) => OK,
        (None, false) => {
            let ids = answer
                .candidates
                .iter()
                .map(|n| n.id.clone())
                .collect::<Vec<_>>()
                .join("`, `");
            let _ = writeln!(
                std::io::stderr(),
                "more than one project owns `{}`: `{ids}`",
                answer.dir.display()
            );
            FOUND_PROBLEMS
        }
        (None, true) => {
            let _ = writeln!(
                std::io::stderr(),
                "{}",
                answer.issue.as_deref().unwrap_or_default()
            );
            FOUND_PROBLEMS
        }
    }
}

fn brief_command(root: &Path, dir: &Path, budget: usize, json: bool) -> Result<u8, Error> {
    match brief::run(root, dir, home().as_deref(), budget) {
        Ok(b) => {
            let output = if json {
                brief::json(&b)
            } else {
                b.human.clone()
            };
            print(&output);
            Ok(OK)
        }
        Err(Error::NoProjectForBrief(message)) => {
            let _ = writeln!(std::io::stderr(), "{message}");
            Ok(FOUND_PROBLEMS)
        }
        Err(e) => Err(e),
    }
}

/// What `--install` or `--check` was asked to do to a file.
enum FileOutcome {
    Created,
    Updated,
    UpToDate,
}

/// What an `--install [DIR]` / `--check [DIR]` pair selected.
enum InstallMode {
    /// Neither flag: print the embedded file.
    Print,
    /// `--install`, into `dir` or the default directory.
    Install(Option<PathBuf>),
    /// `--check`, against `dir` or the default directory.
    Check(Option<PathBuf>),
}

/// Print the skill, or install or check the installed one.
fn skill_command(mode: InstallMode) -> Result<u8, Error> {
    match mode {
        InstallMode::Install(dir) => {
            let dir = skill_dir(dir.as_deref())?;
            install_file(&dir.join(SKILL_FILE), SKILL.as_bytes())?;
            Ok(OK)
        }
        InstallMode::Check(dir) => {
            let dir = skill_dir(dir.as_deref())?;
            check_file(
                &dir.join(SKILL_FILE),
                SKILL.as_bytes(),
                &format!("mnemex skill --install {}", dir.display()),
            )
        }
        InstallMode::Print => {
            print(SKILL);
            Ok(OK)
        }
    }
}

/// Print a hook, or install or check the installed one.
fn extension_command(target: ExtensionTarget, mode: InstallMode) -> Result<u8, Error> {
    let contents = match target {
        ExtensionTarget::Pi => PI_HOOK,
        ExtensionTarget::Claude => CLAUDE_HOOK,
    };
    match mode {
        InstallMode::Install(dir) => {
            let dir = extension_dir(target, dir.as_deref())?;
            let path = dir.join(EXTENSION_FILE);
            install_file(&path, contents.as_bytes())?;
            if matches!(target, ExtensionTarget::Claude) {
                print(&claude_snippet(&path));
            }
            Ok(OK)
        }
        InstallMode::Check(dir) => {
            let dir = extension_dir(target, dir.as_deref())?;
            check_file(
                &dir.join(EXTENSION_FILE),
                contents.as_bytes(),
                &format!(
                    "mnemex extension {} --install {}",
                    target_word(target),
                    dir.display()
                ),
            )
        }
        InstallMode::Print => {
            print(contents);
            Ok(OK)
        }
    }
}

/// The directory a skill lives in: an explicit one, or `~/.agents/skills/mnemex`.
fn skill_dir(explicit: Option<&Path>) -> Result<PathBuf, Error> {
    if let Some(dir) = explicit {
        return Ok(dir.to_path_buf());
    }
    Ok(home()
        .ok_or(Error::NoInstallDirectory)?
        .join(".agents/skills/mnemex"))
}

/// The directory a hook lives in: an explicit one, or the agent's default.
fn extension_dir(target: ExtensionTarget, explicit: Option<&Path>) -> Result<PathBuf, Error> {
    if let Some(dir) = explicit {
        return Ok(dir.to_path_buf());
    }
    match target {
        ExtensionTarget::Pi => {
            let agent = std::env::var_os("PI_CODING_AGENT_DIR").filter(|p| !p.is_empty());
            let agent = match agent {
                Some(dir) => PathBuf::from(dir),
                None => home().ok_or(Error::NoInstallDirectory)?.join(".pi/agent"),
            };
            Ok(agent.join("extensions"))
        }
        ExtensionTarget::Claude => Ok(home()
            .ok_or(Error::NoInstallDirectory)?
            .join(".claude/hooks")),
    }
}

/// Write `contents` to `path`, creating the parent directory, and report what
/// happened.
fn install_file(path: &Path, contents: &[u8]) -> Result<(), Error> {
    match write_if_changed(path, contents)? {
        FileOutcome::Created => print(&format!("created {}\n", path.display())),
        FileOutcome::Updated => print(&format!("updated {}\n", path.display())),
        FileOutcome::UpToDate => print(&format!("up to date {}\n", path.display())),
    }
    Ok(())
}

/// Write `contents` to `path` only when it is missing or differs.
fn write_if_changed(path: &Path, contents: &[u8]) -> Result<FileOutcome, Error> {
    match std::fs::read(path) {
        Ok(existing) if existing == contents => Ok(FileOutcome::UpToDate),
        Ok(_) => {
            std::fs::write(path, contents).map_err(|e| Error::io(path, e))?;
            Ok(FileOutcome::Updated)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            std::fs::write(path, contents).map_err(|e| Error::io(path, e))?;
            Ok(FileOutcome::Created)
        }
        Err(e) => Err(Error::io(path, e)),
    }
}

/// Byte-compare the installed file, exiting 1 when it is missing or stale.
///
/// A read failure other than a missing file is an I/O failure, returned as
/// [`Error::Io`] so `main` reports it as exit 2.
fn check_file(path: &Path, contents: &[u8], hint: &str) -> Result<u8, Error> {
    match std::fs::read(path) {
        Ok(existing) if existing == contents => {
            print(&format!("up to date {}\n", path.display()));
            Ok(OK)
        }
        Ok(_) => {
            let _ = writeln!(
                std::io::stderr(),
                "error: {} is missing or out of date",
                path.display()
            );
            let _ = writeln!(std::io::stderr(), "       run: {hint}");
            Ok(FOUND_PROBLEMS)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let _ = writeln!(
                std::io::stderr(),
                "error: {} is missing or out of date",
                path.display()
            );
            let _ = writeln!(std::io::stderr(), "       run: {hint}");
            Ok(FOUND_PROBLEMS)
        }
        Err(e) => Err(Error::io(path, e)),
    }
}

/// The Claude Code `settings.json` registration, naming `path`.
fn claude_snippet(path: &Path) -> String {
    const TEMPLATE: &str = "register in ~/.claude/settings.json:\n\n{\n  \"hooks\": {\n    \"PostToolUse\": [\n      {\n        \"matcher\": \"Write|Edit\",\n        \"hooks\": [{ \"type\": \"command\", \"command\": \"node {path}\" }]\n      }\n    ],\n    \"Stop\": [\n      {\n        \"hooks\": [{ \"type\": \"command\", \"command\": \"node {path}\" }]\n      }\n    ]\n  }\n}\n";
    TEMPLATE.replace("{path}", &path.display().to_string())
}

/// The shell activation snippet `completion` prints.
fn completion_snippet(shell: CompletionShell) -> &'static str {
    match shell {
        CompletionShell::Bash => "source <(COMPLETE=bash mnemex)",
        CompletionShell::Zsh => "source <(COMPLETE=zsh mnemex)",
        CompletionShell::Fish => "COMPLETE=fish mnemex | source",
        CompletionShell::Elvish => "eval (E:COMPLETE=elvish mnemex | slurp)",
        CompletionShell::Powershell => {
            "$env:COMPLETE = \"powershell\"; mnemex | Out-String | Invoke-Expression; Remove-Item Env:\\COMPLETE"
        }
    }
}

/// The CLI word for a hook target.
fn target_word(target: ExtensionTarget) -> &'static str {
    match target {
        ExtensionTarget::Pi => "pi",
        ExtensionTarget::Claude => "claude",
    }
}
