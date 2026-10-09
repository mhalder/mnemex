//! The whole-vault Obsidian rule.
//!
//! Obsidian renders each frontmatter property by the type recorded in
//! `.obsidian/types.json`. mnemex owns every governed field, so the file is
//! derived state: the expected map is built from the one schema table, and the
//! whole-vault `check` fails when the file disagrees.

use std::collections::BTreeMap;
use std::path::Path;

use crate::diagnostic::{Code, Diagnostic, Severity};
use crate::kind::Kind;

/// The Obsidian property type the schema derives for every field, keyed by
/// field name. One field is covered the moment a kind declares it.
fn derived() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for kind in Kind::ALL {
        for field in kind.spec().fields {
            map.insert(
                field.name.to_owned(),
                field.shape.obsidian_type(field.name).to_owned(),
            );
        }
    }
    map
}

/// Check `<root>/.obsidian/types.json` against the derived map.
///
/// A vault without a `.obsidian/` directory is skipped silently. A vault that
/// has the directory must have a `types.json` that matches exactly: a missing
/// or unreadable file, JSON that does not parse, a document without a `types`
/// object, and each disagreeing property are one error each, sorted by property
/// name so the output is deterministic.
#[must_use]
pub fn check(root: &Path) -> Vec<Diagnostic> {
    let types_path = root.join(".obsidian").join("types.json");
    if !root.join(".obsidian").is_dir() {
        return vec![];
    }
    let types = match read_types(&types_path) {
        Ok(types) => types,
        Err(diagnostic) => return vec![diagnostic],
    };
    mismatches(&types_path, &types)
}

/// The `types` object as it is on disk, or the one file-level diagnostic for a
/// file that is missing, unreadable, unparseable, or without a `types` object.
fn read_types(
    path: &Path,
) -> std::result::Result<serde_json::Map<String, serde_json::Value>, Diagnostic> {
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(mx407(
                path,
                "`.obsidian/types.json` is missing; it must match the property types the schema derives"
                    .to_owned(),
            ));
        }
        Err(e) => {
            return Err(mx407(
                path,
                format!("`.obsidian/types.json` cannot be read: {e}"),
            ));
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&src) {
        Ok(value) => value,
        Err(e) => {
            return Err(mx407(
                path,
                format!("`.obsidian/types.json` is not valid JSON: {e}"),
            ));
        }
    };
    match value.get("types").and_then(serde_json::Value::as_object) {
        Some(types) => Ok(types.clone()),
        None => Err(mx407(
            path,
            "`.obsidian/types.json` has no `types` object; it must map each property to its type"
                .to_owned(),
        )),
    }
}

/// One diagnostic per disagreeing property, sorted by property name.
fn mismatches(path: &Path, types: &serde_json::Map<String, serde_json::Value>) -> Vec<Diagnostic> {
    let expected = derived();
    let mut findings: BTreeMap<String, String> = BTreeMap::new();

    for (name, observed) in types {
        match expected.get(name) {
            Some(want) => {
                let observed = observed_type(observed);
                if observed != *want {
                    findings.insert(name.clone(), wrong_type(name, &observed, want));
                }
            }
            None => {
                findings.insert(name.clone(), unknown(name));
            }
        }
    }
    for (name, want) in &expected {
        if !types.contains_key(name) {
            findings.insert(name.clone(), missing(name, want));
        }
    }

    findings
        .into_values()
        .map(|message| mx407(path, message))
        .collect()
}

/// The type a property is recorded as. Obsidian writes strings; a value that is
/// not a string still cannot match, and is named as its JSON spelling.
fn observed_type(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

fn wrong_type(name: &str, observed: &str, expected: &str) -> String {
    format!("`{name}` must be typed `{expected}`, not `{observed}`")
}

fn unknown(name: &str) -> String {
    format!("`{name}` is not a field the schema declares; remove it from `.obsidian/types.json`")
}

fn missing(name: &str, expected: &str) -> String {
    format!("`{name}` is not typed in `.obsidian/types.json`; the schema derives `{expected}`")
}

fn mx407(path: &Path, message: String) -> Diagnostic {
    Diagnostic {
        path: path.to_path_buf(),
        code: Code::Mx407,
        severity: Severity::Error,
        message,
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_types(root: &Path, contents: &str) {
        let dir = root.join(".obsidian");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("types.json"), contents).expect("write");
    }

    /// Every diagnostic reduced to `(code, message)`, which is all a fixture
    /// asserts about a rule.
    fn findings(root: &Path) -> Vec<(Code, String)> {
        check(root)
            .into_iter()
            .map(|d| (d.code, d.message))
            .collect()
    }

    #[test]
    fn the_derived_map_is_exact() {
        let expected: BTreeMap<String, String> = [
            ("path", "text"),
            ("project", "text"),
            ("refs", "multitext"),
            ("repo", "text"),
            ("status", "text"),
            ("tags", "tags"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        assert_eq!(derived(), expected);
    }

    #[test]
    fn a_vault_without_obsidian_is_skipped() {
        let root = tempfile::tempdir().expect("tempdir");
        assert!(check(root.path()).is_empty());
    }

    #[test]
    fn a_matching_file_passes() {
        let root = tempfile::tempdir().expect("tempdir");
        write_types(
            root.path(),
            r#"{"types":{"path":"text","project":"text","refs":"multitext","repo":"text","status":"text","tags":"tags"}}"#,
        );
        assert!(check(root.path()).is_empty());
    }

    #[test]
    fn a_missing_file_is_one_error() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join(".obsidian")).expect("mkdir");
        let ds = check(root.path());
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].code, Code::Mx407);
        assert_eq!(ds[0].severity, Severity::Error);
        assert_eq!(ds[0].span, None);
        assert!(
            ds[0]
                .message
                .starts_with("`.obsidian/types.json` is missing"),
            "{}",
            ds[0].message
        );
    }

    #[test]
    fn an_unreadable_file_is_one_error() {
        let root = tempfile::tempdir().expect("tempdir");
        // A directory where the file should be reads as an I/O failure rather
        // than a missing file.
        std::fs::create_dir_all(root.path().join(".obsidian").join("types.json")).expect("mkdir");
        let ds = check(root.path());
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].code, Code::Mx407);
        assert!(
            ds[0]
                .message
                .starts_with("`.obsidian/types.json` cannot be read"),
            "{}",
            ds[0].message
        );
    }

    #[test]
    fn an_unparseable_file_is_one_error() {
        let root = tempfile::tempdir().expect("tempdir");
        write_types(root.path(), "not json");
        let ds = check(root.path());
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].code, Code::Mx407);
        assert!(
            ds[0]
                .message
                .starts_with("`.obsidian/types.json` is not valid JSON"),
            "{}",
            ds[0].message
        );
    }

    #[test]
    fn a_document_without_a_types_object_is_one_error() {
        let root = tempfile::tempdir().expect("tempdir");
        write_types(root.path(), r#"{"not_types":{}}"#);
        let ds = check(root.path());
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].code, Code::Mx407);
        assert!(
            ds[0]
                .message
                .starts_with("`.obsidian/types.json` has no `types` object"),
            "{}",
            ds[0].message
        );
    }

    #[test]
    fn an_extra_property_fails_and_is_named() {
        let root = tempfile::tempdir().expect("tempdir");
        write_types(root.path(), r#"{"types":{"status":"text","extra":"text"}}"#);
        let ds = findings(root.path());
        let found = ds
            .iter()
            .find(|(_, m)| m.contains("`extra`"))
            .expect("extra");
        assert_eq!(found.0, Code::Mx407);
        assert!(
            found.1.contains("not a field the schema declares"),
            "{}",
            found.1
        );
    }

    #[test]
    fn a_missing_property_fails_and_is_named() {
        let root = tempfile::tempdir().expect("tempdir");
        write_types(root.path(), r#"{"types":{"status":"text"}}"#);
        let ds = findings(root.path());
        let found = ds.iter().find(|(_, m)| m.contains("`path`")).expect("path");
        assert_eq!(found.0, Code::Mx407);
        assert_eq!(
            found.1,
            "`path` is not typed in `.obsidian/types.json`; the schema derives `text`"
        );
    }

    #[test]
    fn a_wrong_type_fails_and_names_both_types() {
        let root = tempfile::tempdir().expect("tempdir");
        write_types(root.path(), r#"{"types":{"status":"date"}}"#);
        let ds = findings(root.path());
        let found = ds
            .iter()
            .find(|(_, m)| m.contains("`status`"))
            .expect("status");
        assert_eq!(found.1, "`status` must be typed `text`, not `date`");
    }

    #[test]
    fn a_non_string_type_is_named_as_its_json() {
        let root = tempfile::tempdir().expect("tempdir");
        write_types(root.path(), r#"{"types":{"status":3}}"#);
        let ds = findings(root.path());
        let found = ds
            .iter()
            .find(|(_, m)| m.contains("`status`"))
            .expect("status");
        assert_eq!(found.1, "`status` must be typed `text`, not `3`");
    }

    #[test]
    fn multiple_mismatches_are_sorted_by_property_name() {
        let root = tempfile::tempdir().expect("tempdir");
        write_types(
            root.path(),
            r#"{"types":{"path":"text","project":"text","refs":"multitext","repo":"text","status":"date","tags":"checkbox"}}"#,
        );
        let ds = check(root.path());
        assert_eq!(ds.len(), 2);
        assert!(ds[0].message.starts_with("`status`"), "{}", ds[0].message);
        assert!(ds[1].message.starts_with("`tags`"), "{}", ds[1].message);
    }
}
