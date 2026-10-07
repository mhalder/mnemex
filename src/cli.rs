//! The command surface.
//!
//! The derive structs *are* the surface, and the surface is snapshot-tested,
//! because the skill describes these verbs: a rename should surface as a
//! snapshot diff rather than as a skill that quietly lies.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum, ValueHint};
use clap_complete::engine::{ArgValueCandidates, ArgValueCompleter};

use crate::complete;

/// Authors and validates notes in a governed markdown vault.
///
/// The schema lives in the binary and nowhere else: `mnemex schema` prints it.
#[derive(Debug, Parser)]
#[command(name = "mnemex", version, about, long_about = None)]
pub struct Cli {
    /// What to do.
    #[command(subcommand)]
    pub command: Command,
}

/// The verbs.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Mint a note.
    New {
        /// The kind, which is the folder it lands in.
        #[arg(add = ArgValueCandidates::new(complete::kinds))]
        kind: String,
        /// The title. Its slug becomes half the id.
        title: String,
        /// The project this note belongs to.
        #[arg(long, value_name = "PROJECT", add = ArgValueCompleter::new(complete::project))]
        project: Option<String>,
        /// The status, for plan and adr.
        #[arg(long, value_name = "STATUS", add = ArgValueCandidates::new(complete::statuses))]
        status: Option<String>,
        /// A tag. Repeat for more.
        #[arg(long = "tag", value_name = "TAG", add = ArgValueCompleter::new(complete::tag))]
        tags: Vec<String>,
        /// A work-item URL. Repeat for more.
        #[arg(long = "ref", value_name = "URL")]
        refs: Vec<String>,
        /// The directory a project's work happens in. A relative path is taken
        /// from the working directory.
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        path: Option<String>,
        /// The repository a project is. Any spelling of a remote; stored
        /// canonically. Derived from `--path` when that is a checkout.
        #[arg(long, value_name = "REMOTE")]
        repo: Option<String>,
    },
    /// Set a field.
    Set {
        /// The note's id.
        #[arg(add = ArgValueCompleter::new(complete::any_note))]
        note: String,
        /// The field.
        #[arg(add = ArgValueCompleter::new(complete::field))]
        field: String,
        /// The value, or values for a list. A relative `path` is taken from the
        /// working directory.
        #[arg(add = ArgValueCompleter::new(complete::value))]
        values: Vec<String>,
        /// Remove the field instead.
        #[arg(long, conflicts_with = "values")]
        clear: bool,
    },
    /// Retitle a note, and every spoke `project` that names a renamed project.
    Rename {
        /// The note's id.
        #[arg(add = ArgValueCompleter::new(complete::any_note))]
        note: String,
        /// The new title.
        title: String,
        /// Report what would change without writing it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove a note.
    Delete {
        /// The note's id.
        #[arg(add = ArgValueCompleter::new(complete::any_note))]
        note: String,
    },
    /// Bring an existing markdown file into the vault.
    Adopt {
        /// The markdown file to adopt. A stray file in a vault folder is moved,
        /// any other copied.
        #[arg(value_hint = ValueHint::FilePath)]
        file: PathBuf,
        /// The kind, which is the folder it lands in.
        #[arg(long, value_name = "KIND", add = ArgValueCandidates::new(complete::kinds))]
        kind: String,
        /// The project this note belongs to.
        #[arg(long, value_name = "PROJECT", add = ArgValueCompleter::new(complete::project))]
        project: Option<String>,
        /// The status, for plan and adr.
        #[arg(long, value_name = "STATUS", add = ArgValueCandidates::new(complete::statuses))]
        status: Option<String>,
        /// A tag. Repeat for more.
        #[arg(long = "tag", value_name = "TAG", add = ArgValueCompleter::new(complete::tag))]
        tags: Vec<String>,
        /// A work-item URL. Repeat for more.
        #[arg(long = "ref", value_name = "URL")]
        refs: Vec<String>,
    },
    /// Check a note or a whole vault.
    Check {
        /// The note's file to check, not an id.
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        path: Option<PathBuf>,
        /// Emit the versioned envelope.
        #[arg(long)]
        json: bool,
    },
    /// Print the schema. This is the schema.
    Schema {
        /// Emit the versioned envelope.
        #[arg(long)]
        json: bool,
    },
    /// Read a tool-event payload on stdin and report on the write.
    Hook,
    /// List every note, including one whose frontmatter does not parse.
    List {
        /// Restrict to one kind.
        #[arg(long, value_name = "KIND", add = ArgValueCandidates::new(complete::kinds))]
        kind: Option<String>,
        /// Keep only the notes whose `project` names this id.
        #[arg(long, value_name = "PROJECT", add = ArgValueCompleter::new(complete::project))]
        project: Option<String>,
        /// Emit the versioned envelope.
        #[arg(long)]
        json: bool,
    },
    /// Find a note by id.
    Resolve {
        /// The id.
        #[arg(add = ArgValueCompleter::new(complete::any_note))]
        id: String,
        /// Emit the versioned envelope.
        #[arg(long)]
        json: bool,
    },
    /// Find the project that owns a directory.
    Project {
        /// The directory to ask about. Defaults to the working directory.
        #[arg(long = "for", value_name = "DIR", value_hint = ValueHint::DirPath)]
        for_: Option<PathBuf>,
        /// Emit the versioned envelope.
        #[arg(long)]
        json: bool,
    },
    /// What a note belongs to: its project and its project's spokes.
    Show {
        /// The id.
        #[arg(add = ArgValueCompleter::new(complete::any_note))]
        id: String,
        /// Emit the versioned envelope.
        #[arg(long)]
        json: bool,
    },
    /// The one command a session runs at start.
    Brief {
        /// The directory to ask about. Defaults to the working directory.
        #[arg(long = "for", value_name = "DIR", value_hint = ValueHint::DirPath)]
        for_: Option<PathBuf>,
        /// The output budget, in bytes. Defaults to 16384.
        #[arg(long, value_name = "BYTES")]
        budget: Option<usize>,
        /// Emit the versioned envelope.
        #[arg(long)]
        json: bool,
    },
    /// Print the embedded agent skill, or install or check the installed one.
    Skill {
        /// Write the skill to DIR/SKILL.md. Defaults to ~/.agents/skills/mnemex.
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, conflicts_with = "check")]
        install: Option<Option<PathBuf>>,
        /// Exit 1 when DIR/SKILL.md is missing or differs from the embedded skill.
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, conflicts_with = "install")]
        check: Option<Option<PathBuf>>,
    },
    /// Print an embedded hook, or install or check the installed one.
    Extension {
        /// Which agent's hook: Pi's extension or Claude Code's hook.
        target: ExtensionTarget,
        /// Write the hook to DIR/mnemex.ts. Defaults to the agent's extensions or hooks directory.
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, conflicts_with = "check")]
        install: Option<Option<PathBuf>>,
        /// Exit 1 when DIR/mnemex.ts is missing or differs from the embedded hook.
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, conflicts_with = "install")]
        check: Option<Option<PathBuf>>,
    },
    /// Print the shell-completion activation snippet.
    Completion {
        /// The shell whose activation snippet to print.
        shell: CompletionShell,
    },
}

/// Which agent a hook is for.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ExtensionTarget {
    /// The Pi extension.
    Pi,
    /// The Claude Code command hook.
    Claude,
}

/// The shells whose activation snippet `completion` prints.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CompletionShell {
    /// Bash.
    Bash,
    /// Zsh.
    Zsh,
    /// Fish.
    Fish,
    /// Elvish.
    Elvish,
    /// PowerShell.
    Powershell,
}
