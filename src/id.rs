//! Note identity: the composite id.
//!
//! A note's id is its filename stem; nothing in frontmatter records it. The id
//! is twelve ASCII digits of `YYYYMMDDHHMM` **local time**, a hyphen, then a
//! non-empty slug of `[a-z0-9-]`. Because the stamp is fixed width, ids sort
//! lexicographically, which is what lets link lists sort as plain strings.

/// The width of the timestamp prefix.
pub const STAMP_LEN: usize = 12;

/// Slugify a title to at most 40 characters of `[a-z0-9-]`.
///
/// 1. Apostrophes (`'` and `’`) are removed without a separator, so
///    `Dexter's` becomes `dexters`.
/// 2. The title is lowercased.
/// 3. Every maximal run of characters that is not ASCII `a-z` or `0-9` becomes
///    one `-`, with no leading or trailing `-`.
/// 4. A result longer than 40 characters is cut at the last `-` at or before
///    position 40; with none in the first 40 characters, it is cut hard at 40.
/// 5. An empty result is not a valid id half; every verb that mints an id from
///    a title refuses it with [`crate::error::Error::TitleHasNoSlug`].
#[must_use]
pub fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_sep = false;
    for ch in title.chars() {
        if ch == '\'' || ch == '\u{2019}' {
            continue;
        }
        let lowered = ch.to_ascii_lowercase();
        if lowered.is_ascii_lowercase() || lowered.is_ascii_digit() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(lowered);
        } else {
            pending_sep = true;
        }
    }
    if out.len() > 40 {
        // The output is ASCII, so byte index 40 is the fortieth character.
        let cut = out[..40].rfind('-').unwrap_or(40);
        out.truncate(cut);
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}

/// Whether `s` is a valid note id: twelve digits, a hyphen, a non-empty slug.
#[must_use]
pub fn is_valid(s: &str) -> bool {
    let Some((stamp, slug)) = split(s) else {
        return false;
    };
    stamp.len() == STAMP_LEN
        && stamp.bytes().all(|b| b.is_ascii_digit())
        && !slug.is_empty()
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn split(s: &str) -> Option<(&str, &str)> {
    if s.len() <= STAMP_LEN || !s.is_char_boundary(STAMP_LEN) {
        return None;
    }
    let (stamp, rest) = s.split_at(STAMP_LEN);
    let slug = rest.strip_prefix('-')?;
    Some((stamp, slug))
}

/// The twelve-digit local-time stamp of a valid id.
#[must_use]
pub fn stamp(id: &str) -> Option<&str> {
    if is_valid(id) {
        Some(&id[..STAMP_LEN])
    } else {
        None
    }
}

/// The slug half of a valid id: what follows the stamp and its hyphen.
#[must_use]
pub fn slug(id: &str) -> Option<&str> {
    if is_valid(id) {
        split(id).map(|(_, slug)| slug)
    } else {
        None
    }
}
