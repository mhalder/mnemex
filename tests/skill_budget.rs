//! The per-session prose budget.
//!
//! Everything the schema would otherwise say in prose is loaded into every
//! single session, so the tool's real output is a budget. **This is the point of
//! the product:** any implementation that reintroduces a prose copy of the
//! schema has failed, however correct its code.

use std::path::{Path, PathBuf};

use mnemex::kind::Kind;
use mnemex::spec::Shape;

/// The skill's ceiling.
const SKILL_CEILING: usize = 6 * 1024;

fn skill_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/mnemex/SKILL.md")
}

fn skill() -> String {
    std::fs::read_to_string(skill_path()).expect("the skill template")
}

#[test]
fn the_skill_fits_its_ceiling() {
    let bytes = skill().len();
    assert!(
        bytes <= SKILL_CEILING,
        "the skill is {bytes} bytes, over the {SKILL_CEILING} ceiling"
    );
}

#[test]
fn the_skill_names_no_governed_folder() {
    let text = skill();
    for kind in Kind::ALL {
        let folder = format!("{}/", kind.folder());
        assert!(
            !text.contains(&folder),
            "the skill states `{folder}`; `mnemex schema` answers that"
        );
    }
}

#[test]
fn the_skill_writes_no_frontmatter_field() {
    let text = skill();
    for kind in Kind::ALL {
        for f in kind.spec().fields {
            let key = format!("{}:", f.name);
            // The skill's own YAML frontmatter has `name:` and `description:`,
            // neither of which is a schema field.
            assert!(
                !text.contains(&key),
                "the skill writes `{key}`; `mnemex schema` answers that"
            );
        }
    }
}

#[test]
fn the_skill_lists_no_closed_enum() {
    let text = skill();
    for kind in Kind::ALL {
        for f in kind.spec().fields {
            let Shape::Enum(values) = f.shape else {
                continue;
            };
            for pair in values.windows(2) {
                for joiner in [" | ", ", ", " or ", "`, `"] {
                    let listed = pair.join(joiner);
                    assert!(
                        !text.contains(&listed),
                        "the skill lists `{listed}`; `mnemex schema` answers that"
                    );
                }
            }
        }
    }
}

#[test]
fn the_skill_does_not_restate_the_id_format() {
    let text = skill().to_lowercase();
    for copy in ["twelve digits", "yyyymmdd", "%y%m%d", "12 digits"] {
        assert!(
            !text.contains(copy),
            "the skill restates the id format (`{copy}`)"
        );
    }
}

#[test]
fn the_skill_routes_to_the_tool_for_everything_the_tool_answers() {
    let text = skill();
    for verb in [
        "mnemex schema",
        "mnemex show",
        "mnemex resolve",
        "mnemex project",
        "mnemex brief",
    ] {
        assert!(text.contains(verb), "the skill does not route to `{verb}`");
    }
    assert!(
        text.contains("`mnemex schema`"),
        "the skill must say that `mnemex schema` is the schema"
    );
}

#[test]
fn the_skill_tells_the_agent_to_check_its_own_copy() {
    assert!(
        skill().contains("`mnemex skill --check`"),
        "the skill must route the agent to `mnemex skill --check`"
    );
}

#[test]
fn the_skill_names_the_three_admission_rules() {
    let text = skill();
    for rule in [
        "the repo knows it",
        "the README says it",
        "the ADR decides it",
    ] {
        assert!(
            text.contains(rule),
            "the skill does not state the admission rule `{rule}`"
        );
    }
}

#[test]
fn the_readme_does_not_restate_the_schema_either() {
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("README");
    for kind in Kind::ALL {
        for f in kind.spec().fields {
            let Shape::Enum(values) = f.shape else {
                continue;
            };
            for pair in values.windows(2) {
                let listed = pair.join(" | ");
                assert!(!readme.contains(&listed), "the README lists `{listed}`");
            }
        }
    }
}
