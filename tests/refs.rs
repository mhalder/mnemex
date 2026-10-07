//! `refs`: the work-item URL shape.

use mnemex::refs;

#[test]
fn the_examples_table() {
    let cases = [
        (
            "HTTPS://GitHub.com/o/r/pull/42/",
            "https://github.com/o/r/pull/42",
        ),
        (
            "https://github.com:443/o/r/pull/42#files",
            "https://github.com/o/r/pull/42",
        ),
        (
            "https://gitlab.com/g/sub/p/-/merge_requests/7",
            "https://gitlab.com/g/sub/p/-/merge_requests/7",
        ),
        (
            "https://git.example.com/owner/name/pulls/3",
            "https://git.example.com/owner/name/pulls/3",
        ),
        (
            "https://jira.example.com:8443/browse/PROJ-1",
            "https://jira.example.com:8443/browse/PROJ-1",
        ),
        (
            "https://bugs.example.org/show_bug.cgi/?id=42#c3",
            "https://bugs.example.org/show_bug.cgi?id=42",
        ),
    ];
    for (input, want) in cases {
        assert_eq!(refs::canonical(input).as_deref(), Some(want), "{input}");
    }
}

#[test]
fn the_refusal_list() {
    for input in [
        "github.com/o/r/pull/1",                    // no scheme
        "https://user:token@github.com/o/r/pull/1", // credentials
        "https://github.com",                       // no path
        "ftp://github.com/o/r",                     // wrong scheme
        "https://github.com/o/r pull/1",            // whitespace inside
        "https://local/o/r",                        // host is not dotted
        "https://github.com:notaport/o/r",          // non-numeric port
        "https://github.com:/o/r",                  // empty port
    ] {
        assert_eq!(refs::canonical(input), None, "{input}");
    }
}

#[test]
fn canonical_is_idempotent() {
    for input in [
        "HTTPS://GitHub.com/o/r/pull/42/",
        "https://github.com:443/o/r/pull/42#files",
        "https://jira.example.com:8443/browse/PROJ-1",
        "https://bugs.example.org/show_bug.cgi/?id=42#c3",
    ] {
        let once = refs::canonical(input).expect(input);
        assert_eq!(refs::canonical(&once).as_deref(), Some(once.as_str()));
        assert!(refs::is_canonical(&once));
    }
}

#[test]
fn a_default_port_is_dropped_and_a_non_default_one_kept() {
    assert_eq!(
        refs::canonical("https://x.example.com:443/o/r"),
        Some("https://x.example.com/o/r".to_owned())
    );
    assert_eq!(
        refs::canonical("http://x.example.com:80/o/r"),
        Some("http://x.example.com/o/r".to_owned())
    );
    assert_eq!(
        refs::canonical("https://x.example.com:8443/o/r"),
        Some("https://x.example.com:8443/o/r".to_owned())
    );
}

#[test]
fn an_empty_query_is_dropped_but_a_non_empty_one_is_kept() {
    assert_eq!(
        refs::canonical("https://x.example.com/o/r?"),
        Some("https://x.example.com/o/r".to_owned())
    );
    assert_eq!(
        refs::canonical("https://x.example.com/o/r?id=42"),
        Some("https://x.example.com/o/r?id=42".to_owned())
    );
}
