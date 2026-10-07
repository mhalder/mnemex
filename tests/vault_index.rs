//! Vault root and indexing.

mod support;

use std::ffi::OsStr;
use std::path::Path;

use mnemex::index::Index;
use mnemex::kind::Kind;
use mnemex::vault;
use support::Vault;

// --- which vault? -----------------------------------------------------------

#[test]
fn the_vault_root_prefers_memex_vault_then_home() {
    let env = vault::root_from(Some(OsStr::new("/tmp/other")), Some(OsStr::new("/home/x")));
    assert_eq!(env.expect("env"), Path::new("/tmp/other"));

    let home = vault::root_from(None, Some(OsStr::new("/home/x")));
    assert_eq!(home.expect("home"), Path::new("/home/x/mnemex"));

    let none = vault::root_from(None, None);
    assert!(none.is_err());

    // A blank `MNEMEX_VAULT` is the same as unset.
    let blank = vault::root_from(Some(OsStr::new("")), Some(OsStr::new("/home/x")));
    assert_eq!(blank.expect("home"), Path::new("/home/x/mnemex"));
}

// --- index ------------------------------------------------------------------

#[test]
fn governed_files_are_flat_md_sorted_by_kind_then_filename() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-b",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    v.note(
        "plans",
        "202609070815-a",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    v.note("projects", "202609061846-p", "status: active\n");
    v.raw("plans", "not-markdown.txt", "x");
    std::fs::create_dir_all(v.root().join("plans/deep")).expect("mkdir");
    std::fs::write(v.root().join("plans/deep/202609070900-c.md"), "---\n---\n").expect("write");
    v.write("daily", "202609080000-d", "---\n---\n");

    let names: Vec<String> = vault::governed_files(v.root())
        .expect("walk")
        .iter()
        .map(|f| {
            format!(
                "{}/{}",
                f.kind.folder(),
                f.path.file_name().expect("name").display()
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            "projects/202609061846-p.md",
            "plans/202609070815-a.md",
            "plans/202609070816-b.md",
        ]
    );
}

#[test]
fn an_unparseable_note_is_kept_by_id_but_offers_no_frontmatter() {
    let v = Vault::new();
    v.write("plans", "202609070816-p", "no frontmatter here\n");
    v.note(
        "plans",
        "202609070817-q",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let ix = Index::build(v.root()).expect("build");
    assert!(ix.contains("202609070816-p"), "its id still names a note");
    assert!(
        ix.get("202609070816-p").is_none(),
        "but it has no frontmatter to read"
    );
    assert!(ix.contains("202609070817-q"));
    assert_eq!(ix.kind_of("202609070816-p"), Some(Kind::Plan));
}

#[test]
fn a_file_whose_stem_is_not_an_id_is_not_indexed_at_all() {
    let v = Vault::new();
    v.note(
        "plans",
        "notanid",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    let ix = Index::build(v.root()).expect("build");
    assert!(!ix.contains("notanid"));
    assert_eq!(ix.notes().count(), 0);
}

#[test]
fn spokes_of_returns_the_notes_naming_a_project() {
    let v = Vault::new();
    v.note("projects", "202609061846-p", "status: active\n");
    v.note(
        "plans",
        "202609070816-p",
        "project: \"[[202609061846-p]]\"\nstatus: open\n",
    );
    v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: accepted\n",
    );
    v.note(
        "memories",
        "202609061401-m",
        "project: \"[[202609061846-q]]\"\n",
    );
    let ix = Index::build(v.root()).expect("build");

    let ids: Vec<&str> = ix
        .spokes_of("202609061846-p")
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(ids, ["202609070816-p", "202609061400-a"]);
    assert_eq!(ix.spokes_of("202609061846-q").len(), 1);
    assert!(ix.spokes_of("202609061846-gone").is_empty());
}

#[test]
fn a_governed_file_is_an_md_directly_inside_a_governed_folder_of_the_root() {
    let v = Vault::new();
    let note = v.note(
        "adrs",
        "202609061400-a",
        "project: \"[[202609061846-p]]\"\nstatus: accepted\n",
    );
    let file = vault::governed_file(v.root(), &note).expect("a note");
    assert_eq!(
        (file.kind, file.stem.as_str()),
        (Kind::Adr, "202609061400-a")
    );
    // The file itself need not exist, so a missing note can still be reported.
    assert!(vault::governed_file(v.root(), &v.root().join("plans/202609070816-gone.md")).is_some());

    let other = Vault::new();
    for not_a_note in [
        v.root().join("notes/readme.txt"),
        v.root().join("daily/202609080000-d.md"),
        v.root().join("archive/projects/202609061846-p.md"),
        other.root().join("plans/202609070816-p.md"),
        Path::new("202609070816-p.md").to_path_buf(),
    ] {
        assert!(
            vault::governed_file(v.root(), &not_a_note).is_none(),
            "{}",
            not_a_note.display()
        );
    }
}
