//! The round-trip property: parse → emit → parse is
//! idempotent, no key is ever dropped (known or unknown), and body bytes are
//! byte-identical across the round trip.

use std::collections::{BTreeMap, BTreeSet};

use mnemex::frontmatter::{Document, Field, Position, Value, parse, quote};
use mnemex::kind::Kind;
use proptest::prelude::*;

fn key() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z_][a-z0-9_-]{0,8}",
        // Keys outside ASCII, with inner spaces, or that only parse quoted.
        "[a-zа-яё中_][a-zа-яё中0-9 _-]{0,8}",
        Just("a: b".to_owned()),
        Just("# c".to_owned()),
        Just("\"q\"".to_owned()),
        Just("true".to_owned()),
        Just(String::new()),
    ]
}

fn scalar() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        "[ -~]{0,12}",
        Just("[[202609061846-a]]".to_owned()),
        Just("true".to_owned()),
        Just("2026-09-08".to_owned()),
        Just("  padded  ".to_owned()),
        Just("a: b".to_owned()),
        Just("# not a comment".to_owned()),
        // Numeric-looking scalars YAML would decode as numbers; the emitter
        // must quote them so they survive the round trip as text.
        Just("007".to_owned()),
        Just("1e3".to_owned()),
        Just("0x10".to_owned()),
        Just("1.50".to_owned()),
        // Text YAML would read as a null, a bool, an int or a float, and text
        // outside ASCII: each must come back as the text it was.
        Just("null".to_owned()),
        Just("~".to_owned()),
        Just("false".to_owned()),
        Just("-1".to_owned()),
        Just("0.5".to_owned()),
        Just("значение с пробелом".to_owned()),
    ]
}

fn value() -> impl Strategy<Value = Value> {
    prop_oneof![
        scalar().prop_map(Value::Scalar),
        prop::collection::vec(scalar(), 0..4).prop_map(Value::List),
    ]
}

fn document() -> impl Strategy<Value = (Document, Kind)> {
    let known: Vec<String> = Kind::ALL
        .iter()
        .flat_map(|k| k.spec().fields.iter().map(|f| f.name.to_owned()))
        .collect();
    (
        prop::collection::vec(
            (prop_oneof![key(), prop::sample::select(known)], value()),
            0..8,
        ),
        "(?s)[ -~\n\r\t]{0,40}",
        prop::sample::select(Kind::ALL.to_vec()),
    )
        .prop_map(|(pairs, body, kind)| {
            let mut seen = BTreeSet::new();
            let fields = pairs
                .into_iter()
                .filter(|(k, _)| seen.insert(k.clone()))
                .map(|(key, value)| Field {
                    key,
                    key_at: Position::new(1, 1),
                    value,
                    value_at: Position::new(1, 1),
                    items_at: vec![],
                })
                .collect();
            (
                Document {
                    fields,
                    duplicates: vec![],
                    body,
                },
                kind,
            )
        })
}

/// Plain, unquoted frontmatter lines a person might write, under keys no
/// schema knows, with no null spelling among the values.
fn written() -> impl Strategy<Value = (Vec<(String, String)>, Kind)> {
    let text = prop_oneof![
        "[-+]?[0-9][0-9._xeEob]{0,6}",
        "(true|false|True|FALSE|yes|No)",
        "20[0-9]{2}-[01][0-9]-[0-3][0-9]",
        "[a-z][a-z0-9]{0,8}",
    ]
    .prop_filter("a null reads as the absent value", |t| {
        !matches!(t.as_str(), "null" | "Null" | "NULL" | "~")
    });
    (
        prop::collection::vec(("(k|ключ)_[a-z0-9_]{0,6}", text), 1..8),
        prop::sample::select(Kind::ALL.to_vec()),
    )
        .prop_map(|(pairs, kind)| {
            let mut seen = BTreeSet::new();
            let pairs = pairs
                .into_iter()
                .filter(|(k, _)| seen.insert(k.clone()))
                .collect();
            (pairs, kind)
        })
}

fn keys(d: &Document) -> BTreeSet<String> {
    d.fields.iter().map(|f| f.key.clone()).collect()
}

fn values(d: &Document) -> BTreeMap<String, Value> {
    d.fields
        .iter()
        .map(|f| (f.key.clone(), f.value.clone()))
        .collect()
}

proptest! {
    #[test]
    fn parse_emit_parse_is_idempotent((d0, kind) in document()) {
        let s1 = d0.render(kind);
        let d1 = parse(&s1).expect("emitted frontmatter parses");
        let s2 = d1.render(kind);
        let d2 = parse(&s2).expect("emitted frontmatter parses");
        let s3 = d2.render(kind);
        prop_assert_eq!(&s2, &s3, "emission is not idempotent");
        prop_assert_eq!(keys(&d1), keys(&d2), "a key was dropped");
        prop_assert_eq!(keys(&d0), keys(&d1), "a key was dropped on the first pass");
        prop_assert_eq!(&values(&d0), &values(&d1), "a value changed on the first pass");
        prop_assert_eq!(&values(&d1), &values(&d2), "a value changed on the second pass");
        prop_assert_eq!(&d0.body, &d1.body, "body bytes changed");
        prop_assert_eq!(&d1.body, &d2.body, "body bytes changed");
    }

    /// Frontmatter as a person writes it, plain and unquoted, reads back as the
    /// text they wrote and keeps it through every later write.
    #[test]
    fn written_plain_scalars_keep_their_text((pairs, kind) in written()) {
        let mut src = String::from("---\n");
        for (key, text) in &pairs {
            src.push_str(key);
            src.push_str(": ");
            src.push_str(text);
            src.push('\n');
        }
        src.push_str("---\n");
        let d1 = parse(&src).expect("written frontmatter parses");
        let wrote: BTreeMap<String, Value> = pairs
            .into_iter()
            .map(|(k, t)| (k, Value::Scalar(t)))
            .collect();
        prop_assert_eq!(&values(&d1), &wrote, "a written value was decoded");
        let d2 = parse(&d1.render(kind)).expect("emitted frontmatter parses");
        prop_assert_eq!(&values(&d2), &wrote, "a written value changed on a write");
    }

    /// A key stated twice, in any spelling the emitter writes, is found, so no
    /// write keeps only the last of the two.
    #[test]
    fn a_key_stated_twice_is_always_found(k in key()) {
        let spelled = quote(&k);
        let d = parse(&format!("---\n{spelled}: a\n{spelled}: b\n---\n")).expect("parses");
        prop_assert_eq!(
            d.duplicates.iter().map(|(k, p)| (k.clone(), p.line)).collect::<Vec<_>>(),
            vec![(k, 3)]
        );
        prop_assert!(!d.is_renderable());
    }

    #[test]
    fn no_value_is_ever_silently_flattened((d0, kind) in document()) {
        let d1 = parse(&d0.render(kind)).expect("parses");
        // Everything we emit is a scalar or a flat list, so nothing we write is
        // ever read back as unsupported.
        prop_assert!(d1.is_renderable());
    }
}
