//! The one schema table.
//!
//! This module is the single source of truth for canonical field order,
//! requiredness, shape, the kind a link points at, and the value a new note
//! starts with. `check` walks it, `authoring` emits from it, and `schema` prints
//! it. There is no second copy.

use crate::kind::Kind;

/// The shape of a field's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A closed set of values.
    Enum(&'static [&'static str]),
    /// Any scalar.
    Text,
    /// A single `[[id]]`.
    WikiLink,
    /// A list of free-text scalars.
    TextList,
    /// A list of normalised `http(s)://` work-item URLs.
    UrlList,
    /// A git remote, canonically `host/owner/name`.
    Remote,
}

impl Shape {
    /// Whether this shape holds a list rather than a single scalar.
    #[must_use]
    pub fn is_list(self) -> bool {
        matches!(self, Shape::TextList | Shape::UrlList)
    }

    /// Whether values of this shape are `[[id]]` links.
    #[must_use]
    pub fn is_link(self) -> bool {
        matches!(self, Shape::WikiLink)
    }

    /// The shape as `mnemex schema` renders it.
    #[must_use]
    pub fn rendered(self) -> String {
        match self {
            Shape::Enum(vs) => format!("enum: {}", vs.join(" | ")),
            Shape::Text => "text".into(),
            Shape::WikiLink => "wiki-link".into(),
            Shape::TextList => "text list".into(),
            Shape::UrlList => "url list".into(),
            Shape::Remote => "git remote".into(),
        }
    }

    /// The shape's bare name, as the JSON envelope carries it.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Shape::Enum(_) => "enum",
            Shape::Text => "text",
            Shape::WikiLink => "wiki-link",
            Shape::TextList => "text list",
            Shape::UrlList => "url list",
            Shape::Remote => "git remote",
        }
    }
}

/// What `mnemex new` writes for a field.
///
/// Named with a trailing underscore so it does not collide with
/// [`core::default::Default`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Default_ {
    /// Nothing is written. Optional fields are omitted entirely; a required
    /// field with no default is one the tool refuses to guess — `project`,
    /// which `--project` names.
    None,
    /// A literal scalar, which the field accepts as a value.
    Scalar(&'static str),
}

/// One row of the schema table.
#[derive(Clone, Copy, Debug)]
pub struct FieldSpec {
    /// The frontmatter key.
    pub name: &'static str,
    /// Whether the field is required.
    pub required: bool,
    /// The shape of its value.
    pub shape: Shape,
    /// What `new` writes.
    pub default: Default_,
    /// For a link field, the kind of note every link must point at; `None` for
    /// every other field.
    pub target: Option<Kind>,
}

impl FieldSpec {
    /// This field, as a link to notes of `kind`.
    const fn to(self, kind: Kind) -> FieldSpec {
        FieldSpec {
            target: Some(kind),
            ..self
        }
    }
}

/// A kind's fields, in canonical order.
#[derive(Clone, Copy, Debug)]
pub struct KindSpec {
    /// The fields, in canonical order.
    pub fields: &'static [FieldSpec],
}

impl KindSpec {
    /// The row for `name`, if this kind has one.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&'static FieldSpec> {
        self.fields.iter().find(|f| f.name == name)
    }
}

const fn req(name: &'static str, shape: Shape, default: Default_) -> FieldSpec {
    FieldSpec {
        name,
        required: true,
        shape,
        default,
        target: None,
    }
}

const fn opt(name: &'static str, shape: Shape) -> FieldSpec {
    FieldSpec {
        name,
        required: false,
        shape,
        default: Default_::None,
        target: None,
    }
}

const TAGS: FieldSpec = opt("tags", Shape::TextList);
const REFS: FieldSpec = opt("refs", Shape::UrlList);
const PROJECT: FieldSpec = req("project", Shape::WikiLink, Default_::None).to(Kind::Project);

pub(crate) static PROJECT_KIND: KindSpec = KindSpec {
    fields: &[
        req(
            "status",
            Shape::Enum(&["active", "paused", "completed", "archived"]),
            Default_::Scalar("active"),
        ),
        TAGS,
        opt("path", Shape::Text),
        opt("repo", Shape::Remote),
    ],
};

pub(crate) static MEMORY: KindSpec = KindSpec {
    fields: &[PROJECT, TAGS, REFS],
};

pub(crate) static PLAN: KindSpec = KindSpec {
    fields: &[
        PROJECT,
        req(
            "status",
            Shape::Enum(&["open", "done"]),
            Default_::Scalar("open"),
        ),
        TAGS,
        REFS,
    ],
};

pub(crate) static ADR: KindSpec = KindSpec {
    fields: &[
        PROJECT,
        req(
            "status",
            Shape::Enum(&["proposed", "accepted", "deprecated"]),
            Default_::Scalar("proposed"),
        ),
        TAGS,
        REFS,
    ],
};

pub(crate) static CONTEXT: KindSpec = KindSpec {
    fields: &[PROJECT, TAGS],
};

#[cfg(test)]
mod tests {
    use super::*;

    // `req`, `opt` and `to` are private and only ever run in the const
    // evaluation of the tables above, so no integration test executes them at
    // run time.
    #[test]
    fn a_required_field_keeps_its_default_and_an_optional_one_has_none() {
        let r = req("status", Shape::Text, Default_::Scalar("x"));
        assert!(r.required);
        assert_eq!(
            (r.name, r.shape, r.default, r.target),
            ("status", Shape::Text, Default_::Scalar("x"), None)
        );

        let o = opt("path", Shape::Text);
        assert!(!o.required);
        assert_eq!(
            (o.name, o.shape, o.default, o.target),
            ("path", Shape::Text, Default_::None, None)
        );
    }

    #[test]
    fn a_link_field_names_the_kind_it_points_at() {
        let l = PROJECT;
        assert_eq!(
            (l.name, l.required, l.shape, l.target),
            ("project", true, Shape::WikiLink, Some(Kind::Project))
        );
    }

    #[test]
    fn only_text_and_url_lists_are_lists() {
        assert!(Shape::TextList.is_list());
        assert!(Shape::UrlList.is_list());
        assert!(!Shape::WikiLink.is_list());
        assert!(!Shape::Enum(&["open"]).is_list());
    }

    // `to` is a const helper the tables above use at compile time; calling it
    // at run time keeps its body covered.
    #[test]
    fn to_names_the_target_kind_at_run_time_too() {
        let f = opt("x", Shape::WikiLink).to(Kind::Project);
        assert_eq!(f.target, Some(Kind::Project));
        assert_eq!(f.shape, Shape::WikiLink);
        assert!(!f.required);
        assert_eq!(f.default, Default_::None);
        assert_eq!(f.name, "x");
    }

    #[test]
    fn every_shape_renders_and_tags() {
        for shape in [
            Shape::Text,
            Shape::WikiLink,
            Shape::TextList,
            Shape::UrlList,
            Shape::Remote,
        ] {
            assert!(!shape.rendered().is_empty());
            assert!(!shape.tag().is_empty());
        }
        assert_eq!(
            Shape::Enum(&["open", "done"]).rendered(),
            "enum: open | done"
        );
        assert_eq!(Shape::Enum(&["open"]).tag(), "enum");
    }
}
