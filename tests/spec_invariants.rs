//! The schema table is the single source of truth. These assert the shape of
//! the table itself and the invariants around it.

use mnemex::kind::Kind;
use mnemex::spec::{Default_, Shape};

#[test]
fn five_kinds_in_vault_layout_order() {
    let folders: Vec<&str> = Kind::ALL.iter().map(|k| k.folder()).collect();
    assert_eq!(
        folders,
        ["projects", "memories", "plans", "adrs", "contexts"]
    );
}

#[test]
fn kind_names_match_the_table() {
    let names: Vec<&str> = Kind::ALL.iter().map(|k| k.name()).collect();
    assert_eq!(names, ["project", "memory", "plan", "adr", "context"]);
}

#[test]
fn a_folder_resolves_to_its_kind_and_nothing_else_does() {
    for k in Kind::ALL {
        assert_eq!(Kind::from_folder(k.folder()), Some(k));
    }
    assert_eq!(Kind::from_folder("daily"), None);
    assert_eq!(Kind::from_folder("references"), None);
    assert_eq!(Kind::from_folder(""), None);
}

#[test]
fn no_kind_declares_an_id_field() {
    for k in Kind::ALL {
        assert!(
            k.spec().fields.iter().all(|f| f.name != "id"),
            "{} declares an `id` field; identity is the filename",
            k.name()
        );
    }
}

#[test]
fn no_kind_repeats_a_field_name() {
    for k in Kind::ALL {
        let mut seen: Vec<&str> = vec![];
        for f in k.spec().fields {
            assert!(!seen.contains(&f.name), "{} repeats `{}`", k.name(), f.name);
            seen.push(f.name);
        }
    }
}

#[test]
fn only_required_fields_carry_non_none_defaults() {
    for k in Kind::ALL {
        for f in k.spec().fields {
            if !f.required {
                assert!(
                    matches!(f.default, Default_::None),
                    "{}.{} is optional but carries a default",
                    k.name(),
                    f.name
                );
            }
        }
    }
}

#[test]
fn every_default_is_a_value_its_field_would_accept() {
    for k in Kind::ALL {
        for f in k.spec().fields {
            let fits = match (f.default, f.shape) {
                (Default_::None, _) => true,
                (Default_::Scalar(v), Shape::Enum(vals)) => vals.contains(&v),
                (Default_::Scalar(v), Shape::Text) => !v.is_empty(),
                _ => false,
            };
            assert!(
                fits,
                "{}.{}: default {:?} does not fit shape {:?}",
                k.name(),
                f.name,
                f.default,
                f.shape
            );
        }
    }
}

#[test]
fn the_project_table_has_the_expected_shape() {
    let names: Vec<&str> = Kind::Project.spec().fields.iter().map(|f| f.name).collect();
    assert_eq!(names, ["status", "tags", "path", "repo"]);
    assert_eq!(
        Kind::Project.spec().field("status").map(|f| f.shape),
        Some(Shape::Enum(&["active", "paused", "completed", "archived"]))
    );
    assert_eq!(
        Kind::Project.spec().field("repo").map(|f| f.shape),
        Some(Shape::Remote)
    );
}

#[test]
fn every_spoke_has_a_required_project_field_and_nothing_else_does() {
    for k in Kind::ALL {
        let has_project = k.spec().field("project").is_some();
        if k == Kind::Project {
            assert!(!has_project, "a project has no `project` field");
        } else {
            let f = k.spec().field("project").expect("spoke has `project`");
            assert!(f.required);
            assert_eq!(f.shape, Shape::WikiLink);
            assert_eq!(f.target, Some(Kind::Project));
        }
    }
}

#[test]
fn status_enums_are_plan_adr_and_project() {
    assert_eq!(
        Kind::Plan.spec().field("status").map(|f| f.shape),
        Some(Shape::Enum(&["open", "done"]))
    );
    assert_eq!(
        Kind::Adr.spec().field("status").map(|f| f.shape),
        Some(Shape::Enum(&["proposed", "accepted", "deprecated"]))
    );
    assert_eq!(
        Kind::Project.spec().field("status").map(|f| f.shape),
        Some(Shape::Enum(&["active", "paused", "completed", "archived"]))
    );
    assert!(Kind::Memory.spec().field("status").is_none());
    assert!(Kind::Context.spec().field("status").is_none());
}

#[test]
fn refs_is_a_url_list_only_where_the_plan_says() {
    let with_refs: Vec<&str> = Kind::ALL
        .iter()
        .filter(|k| k.spec().field("refs").is_some())
        .map(|k| k.name())
        .collect();
    assert_eq!(with_refs, ["memory", "plan", "adr"]);
    for k in Kind::ALL {
        if let Some(f) = k.spec().field("refs") {
            assert_eq!(f.shape, Shape::UrlList);
            assert!(!f.required);
        }
    }
}

#[test]
fn only_text_and_url_lists_are_lists() {
    assert!(Shape::TextList.is_list());
    assert!(Shape::UrlList.is_list());
    assert!(!Shape::Text.is_list());
    assert!(!Shape::WikiLink.is_list());
}

#[test]
fn the_only_link_field_is_project_pointing_at_project() {
    let mut found = vec![];
    for k in Kind::ALL {
        for f in k.spec().fields {
            assert_eq!(
                f.shape.is_link(),
                f.target.is_some(),
                "{}.{}",
                k.name(),
                f.name
            );
            if let Some(target) = f.target {
                found.push((k.name(), f.name, target.name()));
            }
        }
    }
    assert_eq!(
        found,
        [
            ("memory", "project", "project"),
            ("plan", "project", "project"),
            ("adr", "project", "project"),
            ("context", "project", "project"),
        ]
    );
}
