//! The five kinds and their folders. A note's kind is its folder.
use crate::spec::KindSpec;

/// One of the five governed kinds. The order of the variants is vault-layout
/// order, which is the probe order for id resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// `projects/`
    Project,
    /// `memories/`
    Memory,
    /// `plans/`
    Plan,
    /// `adrs/`
    Adr,
    /// `contexts/`
    Context,
}

impl Kind {
    /// Every kind, in vault-layout order.
    pub const ALL: [Kind; 5] = [
        Kind::Project,
        Kind::Memory,
        Kind::Plan,
        Kind::Adr,
        Kind::Context,
    ];

    /// The folder that defines this kind.
    #[must_use]
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Project => "projects",
            Kind::Memory => "memories",
            Kind::Plan => "plans",
            Kind::Adr => "adrs",
            Kind::Context => "contexts",
        }
    }

    /// The kind's name as `schema` prints it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Kind::Project => "project",
            Kind::Memory => "memory",
            Kind::Plan => "plan",
            Kind::Adr => "adr",
            Kind::Context => "context",
        }
    }

    /// The kind whose folder this is, if any.
    #[must_use]
    pub fn from_folder(folder: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.folder() == folder)
    }

    /// The indefinite article that reads correctly before [`Kind::name`].
    /// `adr` is the only kind whose name opens with a vowel.
    #[must_use]
    pub fn article(self) -> &'static str {
        if self.name().starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        }
    }

    /// This kind's schema row.
    #[must_use]
    pub fn spec(self) -> &'static KindSpec {
        match self {
            Kind::Project => &crate::spec::PROJECT_KIND,
            Kind::Memory => &crate::spec::MEMORY,
            Kind::Plan => &crate::spec::PLAN,
            Kind::Adr => &crate::spec::ADR,
            Kind::Context => &crate::spec::CONTEXT,
        }
    }
}
