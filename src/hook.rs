//! The write hook for tool-event integrations.
//!
//! **The hook always exits 0.** It must never fail the write it is reporting on,
//! so every failure it meets — an unparseable payload, no configured vault, a
//! path that is not a note of that vault, an unreadable file — is normally
//! silence. Integrations may ask for debug notes and decide where to log them.
//!
//! The payload may be a Claude-style `PostToolUse` event (`tool_input.file_path`
//! or `tool_response.file_path`) or the smaller Pi extension event shape
//! (`input.path`). Accepted limit, not a defect: the hook only sees writes made
//! through editing tools. A note changed by a shell redirect or `sed` is not
//! reported until the next `mnemex check`. Two things close that, and both
//! belong outside the binary: the skill denies shell writes into the vault, and
//! the Pi and Claude integrations sweep with `check` at the end of every turn.

use std::path::{Path, PathBuf};

use crate::check::{self, Env};
use crate::report::{self, Summary};
use crate::vault;

/// The prelude a post-write blocking reason opens with.
const BLOCKED: &str = "This post-write check found the vault inconsistent. \
                       Fix it with the `mnemex` verbs, or by editing the body:";

/// The payload sections that can name the written path, in the order they are
/// believed: Claude's `tool_input`, then its `tool_response`, then Pi's `input`.
const SECTIONS: [&str; 3] = ["tool_input", "tool_response", "input"];

/// The keys a section can name the path under, in the order they are believed.
const PATH_KEYS: [&str; 3] = ["path", "file_path", "filePath"];

/// The first string path in `section` of the payload.
///
/// Read leniently, key by key: a section that is not an object, or a sibling
/// key holding a number or a list, is passed over rather than failing the whole
/// payload and hiding a path that is there.
fn path_in(payload: &serde_json::Value, section: &str) -> Option<String> {
    let map = payload.get(section)?.as_object()?;
    PATH_KEYS
        .iter()
        .find_map(|key| map.get(*key)?.as_str().map(ToOwned::to_owned))
}

/// The written path: Claude's `tool_input.file_path`, then
/// `tool_response.file_path` / `tool_response.filePath`, then Pi's `input.path`.
fn written_path(payload: &str) -> Result<String, &'static str> {
    let parsed: serde_json::Value =
        serde_json::from_str(payload).map_err(|_| "payload is not recognized JSON")?;
    SECTIONS
        .iter()
        .find_map(|section| path_in(&parsed, section))
        .ok_or("payload did not name a written path")
}

fn envelope(diagnostics: &[crate::diagnostic::Diagnostic]) -> Option<String> {
    let summary = Summary::of(diagnostics);
    if summary.is_clean() {
        return None;
    }
    let human = report::human(diagnostics);
    let envelope = if summary.has_errors() {
        serde_json::json!({ "decision": "block", "reason": format!("{BLOCKED}\n\n{human}") })
    } else {
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PostToolUse",
                "additionalContext": human,
            }
        })
    };
    Some(envelope.to_string())
}

/// What the hook prints, if anything, for a write judged against the one vault
/// (see [`vault::root`]).
///
/// Errors ask the integration to treat the completed write as blocked; warnings
/// inform; everything else is silence — including a write that is not a note of
/// that vault, and any write at all when no vault is configured.
#[must_use]
pub fn respond(payload: &str, env: Env<'_>) -> Option<String> {
    match vault::root() {
        Ok(root) => respond_at(payload, &root, env).0,
        Err(_) => None,
    }
}

/// What the hook prints plus non-fatal debug notes for an integration log.
///
/// The first tuple item is the normal hook stdout payload. The second is a list
/// of reasons the hook stayed silent or could not run its check.
#[must_use]
pub fn respond_with_debug(payload: &str, env: Env<'_>) -> (Option<String>, Vec<String>) {
    match vault::root() {
        Ok(root) => respond_at(payload, &root, env),
        Err(_) => (
            None,
            vec![
                "no vault is configured: set MNEMEX_VAULT, or keep the vault at ~/mnemex-vault"
                    .to_owned(),
            ],
        ),
    }
}

/// [`respond_with_debug`], against a caller-supplied root, so tests can drive it
/// without mutating the process environment.
#[doc(hidden)]
#[must_use]
pub fn respond_at(payload: &str, root: &Path, env: Env<'_>) -> (Option<String>, Vec<String>) {
    let path = match written_path(payload) {
        Ok(path) => PathBuf::from(path),
        Err(reason) => return (None, vec![reason.to_owned()]),
    };
    if vault::governed_file(root, &path).is_none() {
        return (
            None,
            vec![format!(
                "{} is not a note of the vault at {}",
                path.display(),
                root.display()
            )],
        );
    }
    if !path.is_file() {
        return (None, vec![format!("{} is not a file", path.display())]);
    }
    let diagnostics = match check::path(&path, root, env) {
        Ok(diagnostics) => diagnostics,
        Err(err) => {
            return (
                None,
                vec![format!("could not check {}: {err}", path.display())],
            );
        }
    };
    (envelope(&diagnostics), Vec::new())
}
