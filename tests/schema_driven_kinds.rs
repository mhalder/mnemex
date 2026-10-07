//! Schema-driven kind tests, generated from the one table: for every kind, a
//! minimal-required note is clean, a fully-populated note is clean, dropping
//! any single required field raises `MX100`, omitting any optional field raises
//! nothing, and every enum value in the table is accepted.

mod support;

use mnemex::check::{self, Env};
use mnemex::diagnostic::{Code, Diagnostic};
use mnemex::kind::Kind;
use mnemex::spec::{FieldSpec, Shape};
use support::Vault;

/// The project every spoke's `project` field points at, present in every test
/// vault so `MX202` never fires by accident.
const PROJECT: &str = "202609061846-project";

/// The id of the note under test, per kind.
fn subject(kind: Kind) -> String {
    format!(
        "20260908110{}-subject",
        Kind::ALL.iter().position(|k| *k == kind).unwrap_or(0)
    )
}

/// A value the shape would accept.
fn sample(shape: Shape) -> String {
    match shape {
        Shape::Enum(vs) => (*vs.first().unwrap_or(&"")).to_owned(),
        Shape::Text => "sample".to_owned(),
        Shape::WikiLink => format!("\"[[{PROJECT}]]\""),
        Shape::Remote => "github.com/owner/mnemex".to_owned(),
        Shape::TextList | Shape::UrlList => String::new(),
    }
}

/// One frontmatter line (or block) for a field.
fn emit(kind: Kind, f: &FieldSpec) -> String {
    match f.shape {
        Shape::TextList => format!("{}:\n  - fixture\n", f.name),
        Shape::UrlList => format!("{}:\n  - https://github.com/owner/name/pull/42\n", f.name),
        // A project's `path` must name a directory that exists, or `MX403`
        // has something to say; `workspace` is created in every vault.
        Shape::Text if kind == Kind::Project && f.name == "path" => "path: workspace\n".to_owned(),
        shape => format!("{}: {}\n", f.name, sample(shape)),
    }
}

/// Build a vault holding `kind` with exactly `fields`, plus a project for the
/// spokes to point at, and a workspace directory.
fn vault_with(kind: Kind, fields: &[&FieldSpec]) -> (Vault, Vec<Diagnostic>) {
    let v = Vault::new();
    let id = subject(kind);
    v.dir("workspace");
    v.note("projects", PROJECT, "status: active\n");
    let fm: String = fields.iter().map(|f| emit(kind, f)).collect();
    v.note(kind.folder(), &id, &fm);

    let ds = check::root(v.root(), Env::default()).expect("check");
    let id_md = format!("{id}.md");
    let mine = ds
        .into_iter()
        .filter(|d| d.path.ends_with(&id_md))
        .collect();
    (v, mine)
}

fn required(kind: Kind) -> Vec<&'static FieldSpec> {
    kind.spec().fields.iter().filter(|f| f.required).collect()
}

fn all(kind: Kind) -> Vec<&'static FieldSpec> {
    kind.spec().fields.iter().collect()
}

#[test]
fn a_minimal_required_note_is_clean_for_every_kind() {
    for kind in Kind::ALL {
        let (_v, ds) = vault_with(kind, &required(kind));
        assert!(ds.is_empty(), "{}: {ds:?}", kind.name());
    }
}

#[test]
fn a_fully_populated_note_is_clean_for_every_kind() {
    for kind in Kind::ALL {
        let (_v, ds) = vault_with(kind, &all(kind));
        assert!(ds.is_empty(), "{}: {ds:?}", kind.name());
    }
}

#[test]
fn dropping_any_single_required_field_raises_mx100() {
    for kind in Kind::ALL {
        for dropped in required(kind) {
            let kept: Vec<&FieldSpec> = all(kind)
                .into_iter()
                .filter(|f| f.name != dropped.name)
                .collect();
            let (_v, ds) = vault_with(kind, &kept);
            let hit = ds.iter().any(|d| {
                d.code == Code::Mx100
                    && d.message
                        == format!(
                            "`{}` is required for {} {}",
                            dropped.name,
                            kind.article(),
                            kind.name()
                        )
            });
            assert!(hit, "{} without `{}`: {ds:?}", kind.name(), dropped.name);
        }
    }
}

#[test]
fn omitting_any_optional_field_raises_nothing() {
    for kind in Kind::ALL {
        for dropped in kind.spec().fields.iter().filter(|f| !f.required) {
            let kept: Vec<&FieldSpec> = all(kind)
                .into_iter()
                .filter(|f| f.name != dropped.name)
                .collect();
            let (_v, ds) = vault_with(kind, &kept);
            assert!(
                ds.is_empty(),
                "{} without `{}`: {ds:?}",
                kind.name(),
                dropped.name
            );
        }
    }
}

#[test]
fn every_enum_value_in_the_table_is_accepted() {
    for kind in Kind::ALL {
        for field in kind.spec().fields {
            let Shape::Enum(values) = field.shape else {
                continue;
            };
            for value in values {
                let v = Vault::new();
                v.dir("workspace");
                v.note("projects", PROJECT, "status: active\n");
                let id = subject(kind);
                let fm: String = all(kind)
                    .iter()
                    .map(|f| {
                        if f.name == field.name {
                            format!("{}: {value}\n", f.name)
                        } else {
                            emit(kind, f)
                        }
                    })
                    .collect();
                v.note(kind.folder(), &id, &fm);
                let ds = check::root(v.root(), Env::default()).expect("check");
                assert!(
                    !ds.iter().any(|d| d.code == Code::Mx102),
                    "{}.{} = {value} was refused: {ds:?}",
                    kind.name(),
                    field.name
                );
            }
        }
    }
}
