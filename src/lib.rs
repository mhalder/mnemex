//! `mnemex` — authors and validates notes in a governed markdown vault.
//!
//! The library holds every decision; the binary only parses arguments, prints,
//! and picks an exit code. Integration tests drive this library directly.
//!
//! The one schema table lives in [`spec`] and nowhere else: [`check`] walks it,
//! [`authoring`] emits from it, and [`schema`] prints it.

pub mod authoring;
pub mod brief;
pub mod check;
pub mod cli;
pub mod complete;
pub mod diagnostic;
pub mod error;
pub mod frontmatter;
pub mod git;
pub mod hook;
pub mod id;
pub mod index;
pub mod kind;
pub mod links;
pub mod markdown;
pub mod obsidian;
pub mod path;
pub mod query;
pub mod refs;
pub mod report;
pub mod scaffold;
pub mod schema;
pub mod spec;
pub mod vault;
