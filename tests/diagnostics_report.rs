//! Diagnostic shape, ordering, and the human and JSON renderings.

use mnemex::diagnostic::{Code, Diagnostic, Severity};
use mnemex::frontmatter::Position;
use mnemex::report::{self, Summary};

fn d(code: Code, severity: Severity, span: Option<(usize, usize)>, message: &str) -> Diagnostic {
    Diagnostic {
        path: "/v/adrs/202609061400-a.md".into(),
        code,
        severity,
        message: message.into(),
        span: span.map(|(l, c)| Position::new(l, c)),
    }
}

#[test]
fn codes_are_mx_plus_three_zero_padded_digits() {
    let codes: Vec<&str> = Code::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        codes,
        [
            "MX001", "MX002", "MX003", "MX100", "MX102", "MX104", "MX105", "MX108", "MX110",
            "MX200", "MX202", "MX204", "MX401", "MX403", "MX404", "MX405", "MX406", "MX407",
        ]
    );
}

#[test]
fn eighteen_rules_and_fourteen_holes() {
    assert_eq!(Code::ALL.len(), 18);
    let codes: Vec<&str> = Code::ALL.iter().map(|c| c.as_str()).collect();
    for hole in [
        "MX101", "MX103", "MX106", "MX107", "MX109", "MX201", "MX203", "MX205", "MX206", "MX300",
        "MX301", "MX302", "MX400", "MX402",
    ] {
        assert!(!codes.contains(&hole), "{hole} was reissued");
    }
}

#[test]
fn a_diagnostic_line_carries_path_span_severity_code_and_message() {
    assert_eq!(
        report::line(&d(
            Code::Mx102,
            Severity::Error,
            Some((4, 10)),
            "`status` must be one of"
        )),
        "/v/adrs/202609061400-a.md:4:10: error[MX102]: `status` must be one of"
    );
}

#[test]
fn a_spanless_diagnostic_omits_the_position() {
    assert_eq!(
        report::line(&d(Code::Mx406, Severity::Error, None, "ids must be unique")),
        "/v/adrs/202609061400-a.md: error[MX406]: ids must be unique"
    );
}

#[test]
fn severities_print_as_error_warning_info() {
    assert_eq!(Severity::Error.as_str(), "error");
    assert_eq!(Severity::Warning.as_str(), "warning");
    assert_eq!(Severity::Info.as_str(), "info");
}

#[test]
fn diagnostics_sort_by_span_then_code_with_spanless_first() {
    let mut v = [
        d(Code::Mx104, Severity::Error, Some((4, 1)), "c"),
        d(Code::Mx102, Severity::Error, Some((4, 1)), "b"),
        d(Code::Mx200, Severity::Error, Some((2, 9)), "a"),
        d(Code::Mx406, Severity::Error, None, "spanless"),
    ];
    v.sort_by_key(Diagnostic::sort_key);
    assert_eq!(
        v.iter().map(|x| x.message.as_str()).collect::<Vec<_>>(),
        ["spanless", "a", "b", "c"]
    );
}

#[test]
fn a_summary_pluralises_but_never_pluralises_info_and_omits_zero_counts() {
    let s = |e, w, i| {
        Summary {
            error: e,
            warning: w,
            info: i,
        }
        .line()
    };
    assert_eq!(s(2, 1, 1).as_deref(), Some("2 errors, 1 warning, 1 info"));
    assert_eq!(s(1, 0, 0).as_deref(), Some("1 error"));
    assert_eq!(s(0, 2, 0).as_deref(), Some("2 warnings"));
    assert_eq!(s(0, 0, 3).as_deref(), Some("3 info"));
    assert_eq!(s(0, 0, 0), None);
}

#[test]
fn a_clean_human_report_is_exactly_clean() {
    assert_eq!(report::human(&[]), "clean\n");
}

#[test]
fn a_human_report_is_lines_a_blank_line_then_the_summary() {
    let ds = [
        d(Code::Mx102, Severity::Error, Some((4, 10)), "one"),
        d(Code::Mx105, Severity::Warning, Some((6, 1)), "two"),
    ];
    assert_eq!(
        report::human(&ds),
        "/v/adrs/202609061400-a.md:4:10: error[MX102]: one\n\
         /v/adrs/202609061400-a.md:6:1: warning[MX105]: two\n\
         \n\
         1 error, 1 warning\n"
    );
}

#[test]
fn the_check_human_trailer_is_one_line() {
    assert_eq!(report::check_human(&[], 3), "clean, 3 notes\n");
    assert_eq!(report::check_human(&[], 1), "clean, 1 note\n");
    let ds = [
        d(Code::Mx102, Severity::Error, Some((4, 10)), "one"),
        d(Code::Mx105, Severity::Warning, Some((6, 1)), "two"),
    ];
    assert_eq!(
        report::check_human(&ds, 1),
        "/v/adrs/202609061400-a.md:4:10: error[MX102]: one\n\
         /v/adrs/202609061400-a.md:6:1: warning[MX105]: two\n\
         \n\
         1 error, 1 warning in 1 note\n"
    );
}

#[test]
fn the_check_envelope_is_version_one_and_states_its_absences() {
    let out = report::check_json(&[d(Code::Mx102, Severity::Error, Some((4, 10)), "one")], 1);
    let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(v["version"], 1);
    assert_eq!(v["diagnostics"][0]["path"], "/v/adrs/202609061400-a.md");
    assert_eq!(v["diagnostics"][0]["code"], "MX102");
    assert_eq!(v["diagnostics"][0]["severity"], "error");
    assert_eq!(v["diagnostics"][0]["message"], "one");
    assert_eq!(v["diagnostics"][0]["span"]["line"], 4);
    assert_eq!(v["diagnostics"][0]["span"]["column"], 10);
    assert_eq!(
        v["summary"],
        serde_json::json!({"error": 1, "warning": 0, "info": 0})
    );
    assert!(
        out.ends_with('\n'),
        "json is pretty-printed with a trailing newline"
    );
    assert!(out.contains("\n  "), "json is pretty-printed");
}

#[test]
fn a_clean_check_envelope_carries_empty_diagnostics_not_null() {
    let v: serde_json::Value = serde_json::from_str(&report::check_json(&[], 0)).expect("json");
    assert_eq!(v["diagnostics"], serde_json::json!([]));
    assert_eq!(
        v["summary"],
        serde_json::json!({"error": 0, "warning": 0, "info": 0})
    );
}

#[test]
fn a_spanless_diagnostic_carries_an_explicit_null_span() {
    let v: serde_json::Value = serde_json::from_str(&report::check_json(
        &[d(Code::Mx406, Severity::Error, None, "x")],
        1,
    ))
    .expect("json");
    assert_eq!(v["diagnostics"][0]["span"], serde_json::Value::Null);
    assert!(
        v["diagnostics"][0]
            .as_object()
            .expect("object")
            .contains_key("span")
    );
}
