//! Id grammar and slugification, and id path-safety.

use mnemex::id;

#[test]
fn slugification_lowercases_and_collapses_runs_of_non_alphanumerics() {
    assert_eq!(id::slugify("mnemex"), "mnemex");
    assert_eq!(
        id::slugify("The Rust Programming Language"),
        "the-rust-programming-language"
    );
    assert_eq!(id::slugify("A  --  B"), "a-b");
    assert_eq!(
        id::slugify("  leading and trailing  "),
        "leading-and-trailing"
    );
    assert_eq!(
        id::slugify("C++ / Rust: a comparison!"),
        "c-rust-a-comparison"
    );
    assert_eq!(id::slugify("2026 review"), "2026-review");
    assert_eq!(id::slugify("café über"), "caf-ber");
    assert_eq!(id::slugify("!!!"), "");
    assert_eq!(id::slugify(""), "");
    assert_eq!(id::slugify("-a-"), "a");
}

#[test]
fn apostrophes_are_removed_without_a_separator() {
    assert_eq!(id::slugify("Dexter's"), "dexters");
    assert_eq!(id::slugify("Dexter’s"), "dexters");
    assert_eq!(id::slugify("don't stop"), "dont-stop");
}

#[test]
fn a_title_over_forty_characters_cuts_at_the_last_hyphen() {
    assert_eq!(
        id::slugify("Dexter's vault ontology silently drifts from the memex schema"),
        "dexters-vault-ontology-silently-drifts"
    );
    assert_eq!(
        id::slugify("Fix the dexter review findings and measure retrieval"),
        "fix-the-dexter-review-findings-and"
    );
}

#[test]
fn a_title_without_a_hyphen_in_the_first_forty_characters_is_cut_hard() {
    let slug = id::slugify("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwx");
    assert_eq!(slug.len(), 40);
    assert_eq!(slug, "abcdefghijklmnopqrstuvwxyzabcdefghijklmn");
}

#[test]
fn a_valid_id_is_twelve_digits_a_hyphen_and_a_lowercase_slug() {
    assert!(id::is_valid("202609061846-mnemex"));
    assert!(id::is_valid("202609061846-a"));
    assert!(id::is_valid("000000000000-0"));
    assert!(id::is_valid("202609061846-a-b-c"));
}

#[test]
fn everything_else_is_not_an_id() {
    for bad in [
        "",
        "202609061846",
        "202609061846-",
        "-mnemex",
        "20260906184-mnemex",   // eleven digits
        "2026090618466-mnemex", // thirteen digits
        "202609061846_memex",
        "202609061846-Memex", // uppercase
        "202609061846-memex cli",
        "202609061846-memex.cli",
        "mnemex",
        "20260906184a-memex",
    ] {
        assert!(!id::is_valid(bad), "`{bad}` should not be a valid id");
    }
}

#[test]
fn the_stamp_is_the_first_twelve_characters_of_a_valid_id() {
    assert_eq!(id::stamp("202609061846-mnemex"), Some("202609061846"));
    assert_eq!(id::stamp("not-an-id"), None);
}

#[test]
fn the_slug_is_what_follows_the_stamp_and_its_hyphen() {
    assert_eq!(id::slug("202609061846-mnemex"), Some("mnemex"));
    assert_eq!(id::slug("000000000000-0"), Some("0"));
    assert_eq!(id::slug("mnemex"), None);
    assert_eq!(id::slug("202609061846-"), None);
}

#[test]
fn ids_sort_lexicographically_which_is_chronologically() {
    let mut v = vec!["202609070816-b", "202609061846-a", "202512310000-z"];
    v.sort_unstable();
    assert_eq!(v, ["202512310000-z", "202609061846-a", "202609070816-b"]);
}

#[test]
fn nothing_that_could_escape_the_vault_is_a_valid_id() {
    for bad in [
        "",
        ".",
        "..",
        "../etc/passwd",
        "202609061846-a/b",
        "202609061846-a\\b",
        "202609061846-a\0b",
        ".202609061846-a",
    ] {
        assert!(!id::is_valid(bad), "`{bad}` should not be a valid id");
    }
}
