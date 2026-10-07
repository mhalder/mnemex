//! `mnemex schema` rendering, human and JSON.
//!
//! Both renderings walk the one table in [`crate::spec`]. Anything the tool can
//! answer must not be duplicated in prose: `mnemex schema` *is* the schema.

use core::fmt::Write as _;

use serde::Serialize;

use crate::kind::Kind;
use crate::report::{VERSION, envelope};
use crate::spec::{FieldSpec, Shape};

/// The two lines that say what the table does not.
const PREAMBLE: &str = "A note's kind is its folder, and is never a frontmatter field.\n\
                        Fields are listed in canonical order; `mnemex` emits them this way.\n";

/// The width a field name is padded to.
const NAME_WIDTH: usize = 14;

/// A field's shape as the human rendering prints it: with the kind a link field
/// points at (`wiki-link to project`).
fn shape_line(f: &FieldSpec) -> String {
    let shape = f.shape.rendered();
    match f.target {
        Some(kind) => format!("{shape} to {}", kind.name()),
        None => shape,
    }
}

/// The human rendering.
#[must_use]
pub fn human() -> String {
    let mut out = String::from(PREAMBLE);
    for kind in Kind::ALL {
        let _ = writeln!(out, "\n{} ({}/)", kind.name(), kind.folder());
        let _ = writeln!(out, "  purpose: {}", crate::scaffold::purpose(kind));
        for f in kind.spec().fields {
            let required = if f.required { "required" } else { "optional" };
            let _ = writeln!(
                out,
                "  {:<NAME_WIDTH$}{required}  {}",
                f.name,
                shape_line(f),
                NAME_WIDTH = NAME_WIDTH
            );
        }
        let _ = writeln!(out, "  body:");
        for line in crate::scaffold::sections(kind).lines() {
            if line.is_empty() {
                out.push('\n');
            } else {
                let _ = writeln!(out, "    {line}");
            }
        }
    }
    out
}

#[derive(Serialize)]
struct FieldJson {
    name: &'static str,
    required: bool,
    shape: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<&'static [&'static str]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'static str>,
}

#[derive(Serialize)]
struct KindJson {
    kind: &'static str,
    folder: &'static str,
    purpose: &'static str,
    scaffold: &'static str,
    fields: Vec<FieldJson>,
}

#[derive(Serialize)]
struct SchemaJson {
    version: u32,
    kinds: Vec<KindJson>,
}

/// The JSON envelope. `values` is present only for enums, `target` only for link
/// fields.
#[must_use]
pub fn json() -> String {
    envelope(&SchemaJson {
        version: VERSION,
        kinds: Kind::ALL
            .iter()
            .map(|kind| KindJson {
                kind: kind.name(),
                folder: kind.folder(),
                purpose: crate::scaffold::purpose(*kind),
                scaffold: crate::scaffold::sections(*kind),
                fields: kind
                    .spec()
                    .fields
                    .iter()
                    .map(|f| FieldJson {
                        name: f.name,
                        required: f.required,
                        shape: f.shape.tag(),
                        values: match f.shape {
                            Shape::Enum(v) => Some(v),
                            _ => None,
                        },
                        target: f.target.map(Kind::name),
                    })
                    .collect(),
            })
            .collect(),
    })
}
