//! Work-item URLs: the `refs` shape.
//!
//! A ref is a normalised `http://` or `https://` URL with a path and no
//! credentials. Normalising every spelling of one URL to one value is what lets
//! two notes compare equal, exactly as [`crate::git::canonical`] does for
//! remotes. [`canonical`] is the fixed point: canonicalising a stored value is
//! the value again.

use core::fmt::Write as _;

/// The normalised form of `value`, or `None` when it is not a work-item URL.
///
/// 1. Surrounding whitespace is trimmed; any whitespace inside refuses.
/// 2. The scheme is lowercased and must be `http` or `https`.
/// 3. The fragment (everything from the first `#`) is dropped.
/// 4. The authority holds no credentials: an `@` refuses. A trailing
///    `:<digits>` is the port; a `:` followed by anything else refuses.
/// 5. The port is dropped when it is the scheme's default, kept otherwise.
/// 6. The path must be present: a bare host is a site, not an item.
/// 7. The query is kept verbatim, except an empty one is dropped.
#[must_use]
pub fn canonical(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return None;
    }
    let (scheme, rest) = value.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    // The fragment is dropped before anything else reads the URL, so a `#`
    // inside a query cannot leak it into the path.
    let rest = rest.split_once('#').map_or(rest, |(r, _)| r);

    let (authority, after) = split_at_first_of(rest, &['/', '?']);
    if authority.contains('@') {
        return None;
    }
    let (host, port) = match authority.split_once(':') {
        Some((h, p)) if p.bytes().all(|b| b.is_ascii_digit()) && !p.is_empty() => (h, Some(p)),
        Some(_) => return None,
        None => (authority, None),
    };
    let host = host.to_ascii_lowercase();
    if !crate::git::is_host(&host) {
        return None;
    }
    let port = port.filter(|p| {
        let default = if scheme == "https" { "443" } else { "80" };
        *p != default
    });

    let (path_slashed, query) = after
        .split_once('?')
        .map_or((after, None), |(p, q)| (p, Some(q)));
    let path = path_slashed.trim_end_matches('/');
    if path.is_empty() {
        return None;
    }
    let query = query.filter(|q| !q.is_empty());

    let mut out = format!("{scheme}://{host}");
    if let Some(p) = port {
        let _ = write!(out, ":{p}");
    }
    out.push_str(path);
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    Some(out)
}

/// Whether `value` is already a normalised ref: its own fixed point.
#[must_use]
pub fn is_canonical(value: &str) -> bool {
    canonical(value).as_deref() == Some(value)
}

/// The text before the first of `chars`, and the rest starting at it.
fn split_at_first_of<'a>(s: &'a str, chars: &[char]) -> (&'a str, &'a str) {
    match s.char_indices().find(|(_, c)| chars.contains(c)) {
        Some((i, _)) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent() {
        for value in [
            "HTTPS://GitHub.com/o/r/pull/42/",
            "https://github.com:443/o/r/pull/42#files",
            "https://gitlab.com/g/sub/p/-/merge_requests/7",
            "https://bugs.example.org/show_bug.cgi/?id=42#c3",
        ] {
            let once = canonical(value).expect(value);
            assert_eq!(canonical(&once).as_deref(), Some(once.as_str()));
        }
    }
}
