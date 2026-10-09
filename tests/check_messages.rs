//! Every rule's message and severity, plus rule interaction requirements.

mod support;

use mnemex::check::{self, Env};
use mnemex::diagnostic::{Code, Diagnostic, Severity};
use support::Vault;

/// Check a whole vault and return its diagnostics.
fn run(v: &Vault) -> Vec<Diagnostic> {
    check::root(v.root(), Env::default()).expect("check")
}

/// A diagnostic reduced to what a message test asserts about it.
type Finding = (Code, Severity, String);

fn only(v: &Vault) -> Vec<Finding> {
    run(v)
        .into_iter()
        .map(|d| (d.code, d.severity, d.message))
        .collect()
}

fn codes(v: &Vault) -> Vec<Code> {
    run(v).into_iter().map(|d| d.code).collect()
}

/// A project for spokes to name, so `MX202` never fires by accident.
fn owner(v: &Vault) {
    v.note("projects", "202609061846-owner", "status: active\n");
}

/// A plan naming the owner, so only the rule under test fires.
fn plan(v: &Vault, id: &str, extra: &str) {
    v.note(
        "plans",
        id,
        &format!("project: \"[[202609061846-owner]]\"\nstatus: open\n{extra}"),
    );
}

// --- frontmatter --------------------------------------------------------

#[test]
fn mx001_is_the_parsers_own_message_and_stands_alone() {
    let v = Vault::new();
    v.write("plans", "202609070816-p", "# no frontmatter\n");
    assert_eq!(
        only(&v),
        [(
            Code::Mx001,
            Severity::Error,
            "note does not open with a `---` frontmatter fence".to_owned()
        )]
    );
}

#[test]
fn mx002_names_the_key_and_says_the_last_would_be_kept() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-p",
        "project: \"[[202609061846-owner]]\"\nproject: \"[[202609061846-owner]]\"\nstatus: open\n",
    );
    owner(&v);
    assert_eq!(
        only(&v),
        [(
            Code::Mx002,
            Severity::Error,
            "`project` appears more than once; only the last would be kept".to_owned()
        )]
    );
}

#[test]
fn mx003_names_a_value_the_flat_model_cannot_hold() {
    let v = Vault::new();
    plan(&v, "202609070816-p", "nested:\n  a: 1\n");
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx003,
            Severity::Error,
            "`nested` must be a scalar or a list of scalars".to_owned()
        )),
        "{ds:?}"
    );
}

// --- schema and fields ----------------------------------------------------

#[test]
fn mx100_reports_a_missing_required_field_spanless() {
    let v = Vault::new();
    v.note("memories", "202609081100-m", "");
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx100,
            Severity::Error,
            "`project` is required for a memory".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx100_reports_an_empty_required_scalar_at_the_key() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-p",
        "project: \"[[202609061846-owner]]\"\nstatus:\n",
    );
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx100,
            Severity::Error,
            "`status` is required for a plan".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx102_names_the_enum_and_the_off_value() {
    let v = Vault::new();
    plan(&v, "202609070816-p", "status: maybe\n");
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx102,
            Severity::Error,
            "`status` must be one of `open`, `done`, found `maybe`".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx104_reports_a_scalar_where_a_list_belongs() {
    let v = Vault::new();
    plan(&v, "202609070816-p", "tags: one\n");
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx104,
            Severity::Error,
            "`tags` must be a list, not a single value".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx104_reports_a_list_where_a_scalar_belongs() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-p",
        "project: \"[[202609061846-owner]]\"\nstatus:\n  - open\n",
    );
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx104,
            Severity::Error,
            "`status` must be a single enum, not a list".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx105_names_the_unknown_field_and_that_it_is_preserved() {
    let v = Vault::new();
    plan(&v, "202609070816-p", "owner: me\n");
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx105,
            Severity::Warning,
            "`owner` is not a field of a plan; it is preserved but unchecked".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx108_reports_a_repo_that_is_not_canonical() {
    let v = Vault::new();
    v.note(
        "projects",
        "202609061846-p",
        "status: active\nrepo: git@github.com:a/b.git\n",
    );
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx108,
            Severity::Error,
            "`repo` must be a canonical remote as `host/owner/name`, found `git@github.com:a/b.git`; `mnemex set 202609061846-p repo <url>` canonicalises one".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx110_reports_a_refs_entry_that_is_not_normalised() {
    let v = Vault::new();
    plan(&v, "202609070816-p", "refs:\n  - not a url\n");
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx110,
            Severity::Error,
            "`refs` entry must be a normalised `http(s)://` URL with a path and no credentials, found `not a url`; `mnemex set 202609070816-p refs <url>...` normalises the list".to_owned()
        )),
        "{ds:?}"
    );
}

// --- links ----------------------------------------------------------------

#[test]
fn mx200_reports_a_project_that_is_not_a_wiki_link() {
    let v = Vault::new();
    plan(&v, "202609070816-p", "project: owner\n");
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx200,
            Severity::Error,
            "`project` must be a wiki-link like `[[id]]`, found `owner`".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx202_reports_a_project_naming_a_note_not_in_the_vault() {
    let v = Vault::new();
    plan(&v, "202609070816-p", "project: \"[[202609079999-gone]]\"\n");
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx202,
            Severity::Error,
            "`project` points at `202609079999-gone`, which is not a note in this vault; `mnemex set 202609070816-p project <project-id>` names one".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx204_reports_a_project_naming_a_note_that_is_not_a_project() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070817-q",
        "project: \"[[202609061846-owner]]\"\nstatus: open\n",
    );
    plan(&v, "202609070816-p", "project: \"[[202609070817-q]]\"\n");
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx204,
            Severity::Error,
            "`project` must name a project, but `202609070817-q` is a plan".to_owned()
        )),
        "{ds:?}"
    );
}

// --- layout ---------------------------------------------------------------

#[test]
fn mx401_reports_a_stem_that_is_not_an_id_and_stands_alone() {
    let v = Vault::new();
    v.write("plans", "notanid", "---\n---\n");
    let ds = only(&v);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].0, Code::Mx401);
    assert_eq!(ds[0].1, Severity::Error);
    assert!(
        ds[0]
            .2
            .starts_with("filename `notanid` is not a valid note id"),
        "{}",
        ds[0].2
    );
}

#[test]
fn mx403_reports_a_path_that_is_not_a_directory() {
    let v = Vault::new();
    v.note(
        "projects",
        "202609061846-p",
        "status: active\npath: not-here\n",
    );
    let ds = only(&v);
    assert!(
        ds.iter().any(|(c, s, m)| {
            *c == Code::Mx403
                && *s == Severity::Warning
                && m.contains("which is not a directory here")
        }),
        "{ds:?}"
    );
}

#[test]
fn mx404_reports_a_repo_that_disagrees_with_the_checkout() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:someone/else.git");
    v.note(
        "projects",
        "202609061846-p",
        "status: active\npath: work\nrepo: github.com/owner/mnemex\n",
    );
    let ds = only(&v);
    assert!(
        ds.iter().any(|(c, s, m)| {
            *c == Code::Mx404
                && *s == Severity::Warning
                && m.contains("has remote `github.com/someone/else`")
        }),
        "{ds:?}"
    );
}

#[test]
fn mx405_reports_a_checkout_path_without_a_repo() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:owner/mnemex.git");
    v.note("projects", "202609061846-p", "status: active\npath: work\n");
    let ds = only(&v);
    assert!(
        ds.iter().any(|(c, s, m)| {
            *c == Code::Mx405
                && *s == Severity::Warning
                && m.contains("`mnemex set 202609061846-p repo github.com/owner/mnemex` records it")
        }),
        "{ds:?}"
    );
}

#[test]
fn mx406_reports_one_id_in_more_than_one_folder() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609061846-owner]]\"\nstatus: open\n",
    );
    v.note(
        "adrs",
        "202609070816-x",
        "project: \"[[202609061846-owner]]\"\nstatus: accepted\n",
    );
    owner(&v);
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx406,
            Severity::Error,
            "`202609070816-x` names more than one note (in `plans`, `adrs`); ids must be unique across the vault, so rename one of them with `mnemex rename 202609070816-x <new title>`".to_owned()
        )),
        "{ds:?}"
    );
}

// --- obsidian -------------------------------------------------------------

#[test]
fn mx407_reports_a_wrong_type_naming_observed_and_expected() {
    let v = Vault::new();
    v.raw(
        ".obsidian",
        "types.json",
        r#"{"types":{"path":"text","project":"text","refs":"multitext","repo":"text","status":"date","tags":"tags"}}"#,
    );
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx407,
            Severity::Error,
            "`status` must be typed `text`, not `date`".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx407_reports_an_unknown_property_by_name() {
    let v = Vault::new();
    v.raw(
        ".obsidian",
        "types.json",
        r#"{"types":{"path":"text","project":"text","refs":"multitext","repo":"text","status":"text","tags":"tags","extra":"text"}}"#,
    );
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx407,
            Severity::Error,
            "`extra` is not a field the schema declares; remove it from `.obsidian/types.json`"
                .to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx407_reports_a_missing_property_by_name() {
    let v = Vault::new();
    v.raw(
        ".obsidian",
        "types.json",
        r#"{"types":{"project":"text","refs":"multitext","repo":"text","status":"text","tags":"tags"}}"#,
    );
    let ds = only(&v);
    assert!(
        ds.contains(&(
            Code::Mx407,
            Severity::Error,
            "`path` is not typed in `.obsidian/types.json`; the schema derives `text`".to_owned()
        )),
        "{ds:?}"
    );
}

#[test]
fn mx407_reports_a_missing_file() {
    let v = Vault::new();
    v.dir(".obsidian");
    assert_eq!(
        only(&v),
        [(
            Code::Mx407,
            Severity::Error,
            "`.obsidian/types.json` is missing; it must match the property types the schema derives"
                .to_owned()
        )]
    );
}

#[test]
fn mx407_reports_unparseable_json() {
    let v = Vault::new();
    v.raw(".obsidian", "types.json", "not json");
    let ds = only(&v);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].0, Code::Mx407);
    assert_eq!(ds[0].1, Severity::Error);
    assert!(
        ds[0]
            .2
            .starts_with("`.obsidian/types.json` is not valid JSON"),
        "{}",
        ds[0].2
    );
}

#[test]
fn mx407_reports_a_document_without_a_types_object() {
    let v = Vault::new();
    v.raw(".obsidian", "types.json", r#"{"not_types":{}}"#);
    assert_eq!(
        only(&v),
        [(
            Code::Mx407,
            Severity::Error,
            "`.obsidian/types.json` has no `types` object; it must map each property to its type"
                .to_owned()
        )]
    );
}

// --- interactions ---------------------------------------------------------

#[test]
fn mx108_precedes_mx404_and_mx405() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:someone/else.git");
    v.note(
        "projects",
        "202609061846-p",
        "status: active\npath: work\nrepo: not-a-remote\n",
    );
    let codes = codes(&v);
    assert_eq!(
        codes.iter().filter(|c| **c == Code::Mx108).count(),
        1,
        "{codes:?}"
    );
    assert!(!codes.contains(&Code::Mx404), "{codes:?}");
    assert!(!codes.contains(&Code::Mx405), "{codes:?}");
}

#[test]
fn a_container_error_precedes_its_shape_check() {
    let v = Vault::new();
    v.note("projects", "202609061846-p", "status:\n  - active\n");
    let codes = codes(&v);
    assert_eq!(codes, [Code::Mx104], "no MX102 for a list status");
}

#[test]
fn mx401_reports_alone_even_alongside_other_rules() {
    let v = Vault::new();
    v.write("plans", "notanid", "---\nnested:\n  a: 1\n---\n");
    assert_eq!(codes(&v), [Code::Mx401]);
}
