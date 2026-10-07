//! The one link shape: `[[id]]` and nothing else.
//!
//! There is no display-text form. `[[id|title]]` is not accepted — not read,
//! not written, not normalized — because a title carried inside a link is a
//! cached copy of the target's heading that goes stale silently. It raises
//! `MX200`, which names the bare form to use.

use crate::frontmatter::Value;

/// The target of a well-formed link, or `None` if the value is not one.
///
/// The whole value between the brackets is the target, trimmed; an empty target
/// is not a link, and a `|` anywhere in it is not one either.
#[must_use]
pub fn target(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("[[")?.strip_suffix("]]")?.trim();
    if inner.is_empty() || inner.contains('|') {
        None
    } else {
        Some(inner)
    }
}

/// Wrap a bare id as a link, leaving an already-bracketed value as it stands so
/// that setting twice is idempotent rather than nesting brackets.
#[must_use]
pub fn wrap(value: &str) -> String {
    if value.starts_with("[[") && value.ends_with("]]") {
        value.to_owned()
    } else {
        format!("[[{value}]]")
    }
}

/// Every scalar a link-shaped field holds: one for a `wiki-link`, all of them
/// for a `link list`, none for a value the flat model could not hold.
#[must_use]
pub fn values_of(value: &Value) -> Vec<&str> {
    match value {
        Value::Scalar(s) if s.is_empty() => vec![],
        Value::Scalar(s) => vec![s.as_str()],
        Value::List(items) => items.iter().map(String::as_str).collect(),
        Value::Unsupported => vec![],
    }
}
