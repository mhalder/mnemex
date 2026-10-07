//! Body scaffolds and the two `schema` renderings.

use mnemex::kind::Kind;
use mnemex::{scaffold, schema};

#[test]
fn every_scaffold_opens_with_a_blank_line_and_the_title_heading() {
    for kind in Kind::ALL {
        let b = scaffold::body(kind, "A Title");
        assert!(b.starts_with("\n# A Title\n\n"), "{}: {b:?}", kind.name());
        assert!(b.ends_with('\n'), "{}: {b:?}", kind.name());
    }
}

#[test]
fn the_scaffolds_match_the_kind_table() {
    let sections = |kind: Kind| -> Vec<String> {
        scaffold::body(kind, "T")
            .lines()
            .filter(|l| l.starts_with("## "))
            .map(ToOwned::to_owned)
            .collect()
    };
    assert_eq!(
        sections(Kind::Project),
        ["## Purpose", "## Boundaries", "## Notes"]
    );
    assert_eq!(sections(Kind::Memory), ["## Summary", "## Details"]);
    assert_eq!(sections(Kind::Plan), ["## Goal", "## Steps"]);
    assert_eq!(
        sections(Kind::Adr),
        [
            "## Context",
            "## Decision",
            "## Considered options",
            "## Consequences"
        ]
    );
    assert_eq!(
        sections(Kind::Context),
        [
            "## Summary",
            "## Language",
            "## Relationships",
            "## Example dialogue",
            "## Flagged ambiguities"
        ]
    );
}

#[test]
fn a_plan_scaffold_carries_an_unchecked_box() {
    assert_eq!(
        scaffold::body(Kind::Plan, "T"),
        "\n# T\n\n## Goal\n\n## Steps\n\n- [ ] \n"
    );
}

#[test]
fn a_project_scaffold_is_purpose_boundaries_notes() {
    assert_eq!(
        scaffold::body(Kind::Project, "T"),
        "\n# T\n\n## Purpose\n\n## Boundaries\n\n## Notes\n"
    );
}

#[test]
fn schema_prints_each_kinds_purpose_and_body_scaffold() {
    let out = schema::human();
    for kind in Kind::ALL {
        assert!(
            out.contains(scaffold::purpose(kind)),
            "{} has no purpose",
            kind.name()
        );
        for heading in scaffold::sections(kind)
            .lines()
            .filter(|l| l.starts_with("## "))
        {
            assert!(
                out.contains(&format!("    {heading}")),
                "{} is missing `{heading}`",
                kind.name()
            );
        }
    }
    let v: serde_json::Value = serde_json::from_str(&schema::json()).expect("json");
    let kinds = v["kinds"].as_array().expect("kinds");
    assert_eq!(kinds[0]["purpose"], scaffold::purpose(Kind::Project));
    assert_eq!(kinds[0]["scaffold"], scaffold::sections(Kind::Project));
}

#[test]
fn schema_opens_with_the_two_line_preamble() {
    let out = schema::human();
    let mut lines = out.lines();
    assert_eq!(
        lines.next(),
        Some("A note's kind is its folder, and is never a frontmatter field.")
    );
    assert_eq!(
        lines.next(),
        Some("Fields are listed in canonical order; `mnemex` emits them this way.")
    );
}

#[test]
fn schema_prints_every_kind_its_folder_and_every_field() {
    let out = schema::human();
    for kind in Kind::ALL {
        assert!(
            out.contains(&format!("{} ({}/)", kind.name(), kind.folder())),
            "{} is missing",
            kind.name()
        );
        for f in kind.spec().fields {
            assert!(
                out.contains(f.name),
                "{}.{} is missing",
                kind.name(),
                f.name
            );
        }
    }
    assert!(out.contains("  status        required  enum: active | paused | completed | archived"));
    assert!(out.contains("  project       required  wiki-link to project"));
    assert!(out.contains("  refs          optional  url list"));
    assert!(
        !out.contains("id "),
        "identity is the filename, not a field"
    );
}

#[test]
fn the_schema_envelope_is_the_same_table() {
    let v: serde_json::Value = serde_json::from_str(&schema::json()).expect("json");
    assert_eq!(v["version"], 1);
    let kinds = v["kinds"].as_array().expect("kinds");
    assert_eq!(kinds.len(), 5);
    assert_eq!(kinds[0]["kind"], "project");
    assert_eq!(kinds[0]["folder"], "projects");
    assert_eq!(kinds[0]["fields"][0]["name"], "status");
    assert_eq!(kinds[0]["fields"][0]["required"], true);
    assert_eq!(kinds[0]["fields"][0]["shape"], "enum");
    assert_eq!(
        kinds[0]["fields"][0]["values"],
        serde_json::json!(["active", "paused", "completed", "archived"])
    );
    // `values` is present only for enums.
    assert!(
        kinds[0]["fields"][1]
            .as_object()
            .expect("obj")
            .get("values")
            .is_none()
    );
    assert_eq!(kinds[0]["fields"][1]["shape"], "text list");
    // `project` points at a project.
    assert_eq!(kinds[1]["fields"][0]["name"], "project");
    assert_eq!(kinds[1]["fields"][0]["target"], "project");
    assert_eq!(kinds[1]["fields"][0]["shape"], "wiki-link");
}
