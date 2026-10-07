//! The frontmatter engine: fences, spans, duplicates, typed scalars,
//! unsupported values, and canonical emission.

use mnemex::frontmatter::{self, Document, Value, parse};
use mnemex::kind::Kind;

fn doc(src: &str) -> Document {
    parse(src).expect("parses")
}

// --- parsing ----------------------------------------------------------------

#[test]
fn a_note_opens_with_a_fence_and_the_body_is_captured_verbatim() {
    let d = doc("---\nstatus: active\n---\n\n# Title\n\nbody\n");
    assert_eq!(d.body, "\n# Title\n\nbody\n");
    assert_eq!(
        d.get("status").map(|f| f.value.clone()),
        Some(Value::Scalar("active".into()))
    );
}

#[test]
fn a_crlf_opening_fence_is_accepted() {
    let d = doc("---\r\nstatus: active\n---\nbody");
    assert_eq!(
        d.get("status").map(|f| f.value.clone()),
        Some(Value::Scalar("active".into()))
    );
    assert_eq!(d.body, "body");
}

#[test]
fn no_opening_fence_is_a_parse_error_at_one_one() {
    let e = parse("# Title\n").expect_err("no fence");
    assert_eq!(
        e.message,
        "note does not open with a `---` frontmatter fence"
    );
    assert_eq!((e.at.line, e.at.column), (1, 1));
}

#[test]
fn no_closing_fence_is_a_parse_error_at_one_one() {
    let e = parse("---\nstatus: active\n").expect_err("never closed");
    assert_eq!(
        e.message,
        "frontmatter block is never closed by a `---` fence"
    );
    assert_eq!((e.at.line, e.at.column), (1, 1));
}

#[test]
fn a_block_that_is_not_a_mapping_is_a_parse_error() {
    let e = parse("---\n- a\n- b\n---\n").expect_err("not a mapping");
    assert_eq!(e.message, "frontmatter must be a mapping");
}

#[test]
fn a_non_string_key_is_a_parse_error() {
    let e = parse("---\n1: a\n---\n").expect_err("non-string key");
    assert_eq!(
        e.message,
        "frontmatter keys must be strings; quote `1` as `\"1\"`"
    );
    let e = parse("---\n? [a]\n: b\n---\n").expect_err("a sequence key");
    assert_eq!(e.message, "frontmatter keys must be strings");
}

#[test]
fn a_second_yaml_document_in_the_block_is_a_parse_error() {
    let e = parse("---\nstatus: active\n...\ntags:\n  - a\n---\n").expect_err("two documents");
    assert_eq!(
        e.message,
        "frontmatter holds a second YAML document after `...`; everything after it would be lost, so move those fields above it"
    );
    assert_eq!((e.at.line, e.at.column), (4, 1));

    // A document end marker with nothing after it is still one document.
    let d = doc("---\nstatus: active\n...\n---\n");
    assert_eq!(
        d.get("status").map(|f| f.value.clone()),
        Some(Value::Scalar("active".into()))
    );
}

#[test]
fn an_empty_block_is_well_formed_and_carries_no_fields() {
    let d = doc("---\n---\nbody\n");
    assert!(d.fields.is_empty());
    assert_eq!(d.body, "body\n");
}

#[test]
fn a_yaml_scan_error_surfaces_with_the_parsers_own_position() {
    let e = parse("---\na: [1\nb: 2\n---\n").expect_err("scan error");
    // MX001 carries the parser's own message at the parser's own position.
    assert_eq!(e.message, "illegal placement of ':' indicator");
    assert_eq!(
        e.at.line, 3,
        "position should be inside the block, got {:?}",
        e.at
    );
}

// --- spans ------------------------------------------------------------------

#[test]
fn spans_are_one_based_whole_file_coordinates() {
    let d = doc("---\nstatus: active\ntags:\n  - a\n  - b\n---\n");
    let status = d.get("status").expect("status");
    assert_eq!((status.key_at.line, status.key_at.column), (2, 1));
    assert_eq!((status.value_at.line, status.value_at.column), (2, 9));
    let tags = d.get("tags").expect("tags");
    assert_eq!((tags.key_at.line, tags.key_at.column), (3, 1));
    assert_eq!(
        tags.items_at
            .iter()
            .map(|p| (p.line, p.column))
            .collect::<Vec<_>>(),
        [(4, 5), (5, 5)]
    );
}

#[test]
fn an_empty_scalar_anchors_to_its_key_and_everything_else_to_its_value() {
    let d = doc("---\nurl:\nstatus: active\n---\n");
    let url = d.get("url").expect("url");
    assert_eq!(url.value, Value::Scalar(String::new()));
    assert_eq!((url.anchor().line, url.anchor().column), (2, 1));
    let status = d.get("status").expect("status");
    assert_eq!((status.anchor().line, status.anchor().column), (3, 9));
}

// --- duplicates -------------------------------------------------------------

#[test]
fn duplicate_keys_are_found_by_the_pre_scan_and_the_last_value_is_kept() {
    let d = doc("---\nstatus: active\ntags:\n  - a\nstatus: paused\n---\n");
    assert_eq!(
        d.duplicates
            .iter()
            .map(|(k, p)| (k.as_str(), p.line))
            .collect::<Vec<_>>(),
        [("status", 5)]
    );
    assert_eq!(
        d.get("status").map(|f| f.value.clone()),
        Some(Value::Scalar("paused".into()))
    );
}

#[test]
fn a_quoted_key_is_the_same_key_to_the_pre_scan() {
    let d = doc(
        "---\ntags:\n  - a\n\"tags\":\n  - b\n'status': x\nstatus: y\n\"odd key\": 1\n'odd key': 2\n---\n",
    );
    assert_eq!(
        d.duplicates
            .iter()
            .map(|(k, p)| (k.as_str(), p.line))
            .collect::<Vec<_>>(),
        [("tags", 4), ("status", 7), ("odd key", 9)]
    );
    assert!(!d.is_renderable(), "a write would keep only one of each");
}

#[test]
fn a_repeated_key_outside_ascii_is_reported_and_refused() {
    let d = doc("---\nключ: 1\nstatus: active\nключ: 2\n\"a b\": x\na b: y\n---\n");
    assert_eq!(
        d.duplicates
            .iter()
            .map(|(k, p)| (k.as_str(), p.line))
            .collect::<Vec<_>>(),
        [("ключ", 4), ("a b", 6)]
    );
    assert!(
        !d.is_renderable(),
        "a write would keep only the last of each"
    );
}

#[test]
fn the_pre_scan_ignores_indented_comment_and_implausible_lines() {
    let d = doc("---\ntags:\n  - a\n  - a\n# status: x\n# status: x\nstatus: active\n---\n");
    assert!(d.duplicates.is_empty(), "{:?}", d.duplicates);
}

// --- typed scalars ----------------------------------------------------------

#[test]
fn typed_scalars_read_back_as_their_frontmatter_text() {
    let d = doc("---\nb: true\nc: false\ni: 42\nf: 1.5\nn: null\ne:\ns: 2026-09-08\n---\n");
    let text = |k: &str| match &d.get(k).expect("field").value {
        Value::Scalar(s) => s.clone(),
        other => panic!("{k} is {other:?}"),
    };
    assert_eq!(text("b"), "true");
    assert_eq!(text("c"), "false");
    assert_eq!(text("i"), "42");
    assert_eq!(text("f"), "1.5");
    assert_eq!(text("n"), "");
    assert_eq!(text("e"), "");
    assert_eq!(text("s"), "2026-09-08");
}

#[test]
fn a_plain_scalar_keeps_the_text_it_was_written_with() {
    let d = doc(
        "---\nratio: 1.50\nhex: 0x10\nsci: 1e3\npadded: 007\nsigned: +1\ntilde: ~\nword: Null\nlist:\n  - 1.50\n  - 0o17\n---\n",
    );
    let text = |k: &str| d.get(k).expect("field").value.clone();
    assert_eq!(text("ratio"), Value::Scalar("1.50".into()));
    assert_eq!(text("hex"), Value::Scalar("0x10".into()));
    assert_eq!(text("sci"), Value::Scalar("1e3".into()));
    assert_eq!(text("padded"), Value::Scalar("007".into()));
    assert_eq!(text("signed"), Value::Scalar("+1".into()));
    // A null is still the absent value, however it is spelled.
    assert_eq!(text("tilde"), Value::Scalar(String::new()));
    assert_eq!(text("word"), Value::Scalar(String::new()));
    assert_eq!(
        text("list"),
        Value::List(vec!["1.50".into(), "0o17".into()])
    );
}

#[test]
fn a_block_that_is_only_a_null_carries_no_fields() {
    for block in ["~", "null"] {
        let d = doc(&format!("---\n{block}\n---\nbody\n"));
        assert!(d.fields.is_empty(), "{block}");
        assert_eq!(d.body, "body\n", "{block}");
    }
    let e = parse("---\n\"~\"\n---\n").expect_err("a quoted `~` is text");
    assert_eq!(e.message, "frontmatter must be a mapping");
}

#[test]
fn a_carriage_return_and_other_control_characters_are_escaped() {
    assert_eq!(frontmatter::quote("a\rb"), "\"a\\rb\"");
    assert_eq!(frontmatter::quote("a\u{1}b"), "\"a\\u0001b\"");
}

// --- unsupported ------------------------------------------------------------

#[test]
fn nested_values_are_unsupported_not_flattened() {
    let d = doc("---\nnest:\n  a: 1\nlist:\n  - x: 1\nok: 1\n---\n");
    assert_eq!(
        d.get("nest").map(|f| f.value.clone()),
        Some(Value::Unsupported)
    );
    assert_eq!(
        d.get("list").map(|f| f.value.clone()),
        Some(Value::Unsupported)
    );
    assert!(!d.is_renderable());
}

#[test]
fn a_bare_wiki_link_is_a_nested_flow_sequence_and_so_unsupported() {
    let d = doc("---\nplans:\n  - [[202609061846-a]]\n---\n");
    assert_eq!(
        d.get("plans").map(|f| f.value.clone()),
        Some(Value::Unsupported)
    );
}

#[test]
fn a_document_of_scalars_and_flat_lists_is_renderable() {
    assert!(doc("---\na: 1\nb:\n  - x\n  - y\nc: []\n---\n").is_renderable());
}

// --- canonical emission -----------------------------------------------------

#[test]
fn a_key_that_opens_like_a_document_marker_is_quoted() {
    let d = doc("---\n\"--- a\": x\n\"... b\": y\n---\n");
    let rendered = d.render(Kind::Context);
    assert!(rendered.contains("\n\"--- a\": x\n"), "{rendered}");
    assert!(rendered.contains("\n\"... b\": y\n"), "{rendered}");
    let back = parse(&rendered).expect("rendered frontmatter parses");
    assert_eq!(
        back.fields
            .iter()
            .map(|f| f.key.as_str())
            .collect::<Vec<_>>(),
        ["--- a", "... b"]
    );
}

#[test]
fn emission_puts_known_fields_in_canonical_order_then_unknowns_as_read() {
    let d = doc("---\nzz: 1\nrepo: github.com/a/b\nstatus: active\npath: work\naa: 2\n---\nbody\n");
    assert_eq!(
        d.render(Kind::Project),
        "---\nstatus: active\npath: work\nrepo: github.com/a/b\nzz: \"1\"\naa: \"2\"\n---\nbody\n"
    );
}

#[test]
fn the_four_emission_rows_of_the_value_table() {
    let d = doc("---\nempty:\nscalar: text\nemptylist: []\nlist:\n  - a\n  - b\n---\n");
    assert_eq!(
        d.render(Kind::Context),
        "---\nempty:\nscalar: text\nemptylist: []\nlist:\n  - a\n  - b\n---\n"
    );
}

#[test]
fn quoting_covers_every_case_the_spec_names() {
    let cases = [
        ("[[202609061846-a]]", "\"[[202609061846-a]]\""),
        ("{a}", "\"{a}\""),
        (">fold", "\">fold\""),
        ("|pipe", "\"|pipe\""),
        ("*star", "\"*star\""),
        ("&amp", "\"&amp\""),
        ("!bang", "\"!bang\""),
        ("%pct", "\"%pct\""),
        ("@at", "\"@at\""),
        ("`tick", "\"`tick\""),
        ("'quote", "\"'quote\""),
        ("\"dquote", "\"\\\"dquote\""),
        ("#hash", "\"#hash\""),
        (",comma", "\",comma\""),
        ("- dash", "\"- dash\""),
        (": colon", "\": colon\""),
        ("a: b", "\"a: b\""),
        ("ends:", "\"ends:\""),
        ("a #c", "\"a #c\""),
        (" lead", "\" lead\""),
        ("trail ", "\"trail \""),
        // A backslash is not a quoting trigger; plain YAML reads it literally.
        ("back\\slash", "back\\slash"),
        // ... but inside quotes it is escaped, as the spec says.
        ("a: b\\c", "\"a: b\\\\c\""),
        ("plain", "plain"),
        ("a-b", "a-b"),
        ("a#b", "a#b"),
        // A lone `-` opens a block sequence where it is a value, so it quotes.
        ("-", "\"-\""),
        ("-x", "-x"),
    ];
    for (raw, want) in cases {
        let src = format!(
            "---\nt:\n  - \"{}\"\n---\n",
            raw.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let rendered = doc(&src).render(Kind::Context);
        assert_eq!(
            rendered,
            format!("---\nt:\n  - {want}\n---\n"),
            "raw {raw:?}"
        );
    }
}

#[test]
fn an_empty_list_item_is_quoted() {
    assert_eq!(
        doc("---\nt:\n  - \"\"\n---\n").render(Kind::Context),
        "---\nt:\n  - \"\"\n---\n"
    );
}

/// A scalar that would parse as something other than a string keeps its text
/// quoted, so the round trip the schema requires holds.
#[test]
fn a_non_string_scalar_keeps_its_text() {
    let rendered = doc("---\nconsumed: \"true\"\n---\n").render(Kind::Context);
    assert_eq!(rendered, "---\nconsumed: \"true\"\n---\n");
    assert_eq!(
        doc("---\nconsumed: false\n---\n").render(Kind::Context),
        "---\nconsumed: \"false\"\n---\n"
    );
    assert_eq!(
        doc("---\nconsumed: True\n---\n").render(Kind::Context),
        "---\nconsumed: \"True\"\n---\n"
    );
}

#[test]
fn render_is_idempotent_and_keeps_body_bytes() {
    let src = "---\nzz: 1\nstatus: active\nplans:\n  - \"[[202609061846-a]]\"\n---\n\n# T\n\nbody\r\nwith crlf\n";
    let once = doc(src).render(Kind::Project);
    let twice = doc(&once).render(Kind::Project);
    assert_eq!(once, twice);
    assert!(once.ends_with("\n# T\n\nbody\r\nwith crlf\n"));
}
