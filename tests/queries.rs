//! The queries: `resolve`, `list`, `project` and `show`.

mod support;

use mnemex::kind::Kind;
use mnemex::query;
use support::Vault;

fn seed(v: &Vault) {
    v.write(
        "projects",
        "202609061846-mnemex",
        "---\nstatus: active\npath: work\nrepo: github.com/owner/mnemex\n---\n\n# mnemex\n\nbody\n",
    );
    v.write(
        "plans",
        "202609070816-write-enforcement",
        "---\nproject: \"[[202609061846-mnemex]]\"\nstatus: open\ntags:\n  - x\n---\n\n# mnemex write enforcement\n",
    );
    v.write(
        "adrs",
        "202609061400-ownership",
        "---\nproject: \"[[202609061846-mnemex]]\"\nstatus: accepted\n---\n\n# Frontmatter is tool-owned\n",
    );
}

// --- resolve ---------------------------------------------------------------

#[test]
fn resolve_finds_a_note_its_kind_title_and_fields() {
    let v = Vault::new();
    seed(&v);
    let r = query::resolve(v.root(), "202609061400-ownership").expect("found");
    assert_eq!(r.kind, Kind::Adr);
    assert_eq!(r.title, "Frontmatter is tool-owned");
    assert_eq!(r.path, v.root().join("adrs/202609061400-ownership.md"));
    let fields = r.fields.expect("fields");
    assert_eq!(fields[0].key, "project");
    assert_eq!(fields[0].value.as_scalar(), Some("[[202609061846-mnemex]]"));
    assert_eq!(fields[1].key, "status");
    assert_eq!(fields[1].value.as_scalar(), Some("accepted"));
}

#[test]
fn the_title_is_the_first_heading_past_the_frontmatter() {
    let v = Vault::new();
    v.write(
        "memories",
        "202609080000-n",
        "---\n# not a heading\nproject: \"[[202609061846-p]]\"\n---\n\n# The Real Title\n\n# A second heading\n",
    );
    assert_eq!(
        query::resolve(v.root(), "202609080000-n")
            .expect("found")
            .title,
        "The Real Title"
    );
}

#[test]
fn a_note_with_no_heading_renders_as_untitled() {
    let v = Vault::new();
    v.write(
        "memories",
        "202609080000-n",
        "---\nproject: \"[[202609061846-p]]\"\n---\n\njust prose\n",
    );
    assert_eq!(
        query::resolve(v.root(), "202609080000-n")
            .expect("found")
            .title,
        "(untitled)"
    );
}

#[test]
fn resolve_finds_nothing_for_an_unknown_or_invalid_id() {
    let v = Vault::new();
    seed(&v);
    assert_eq!(query::resolve(v.root(), "202609079999-gone"), None);
    for bad in ["", ".", "..", "../../etc/passwd", "a/b", "a\\b", ".hidden"] {
        assert_eq!(query::resolve(v.root(), bad), None, "`{bad}` resolved");
    }
}

// --- list ------------------------------------------------------------------

#[test]
fn list_returns_every_indexed_note_in_index_order() {
    let v = Vault::new();
    seed(&v);
    let notes = query::list(v.root(), None, None).expect("list");
    assert_eq!(
        notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
        [
            "202609061846-mnemex",
            "202609070816-write-enforcement",
            "202609061400-ownership",
        ]
    );
}

#[test]
fn list_filters_to_one_kind() {
    let v = Vault::new();
    seed(&v);
    let notes = query::list(v.root(), Some(Kind::Plan), None).expect("list");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].id, "202609070816-write-enforcement");
}

#[test]
fn list_project_keeps_the_spokes_and_excludes_the_project() {
    let v = Vault::new();
    seed(&v);
    let notes = query::list(v.root(), None, Some("202609061846-mnemex")).expect("list");
    assert_eq!(
        notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
        ["202609070816-write-enforcement", "202609061400-ownership"]
    );
}

#[test]
fn list_does_not_hide_an_unparseable_note() {
    let v = Vault::new();
    let path = v.write(
        "plans",
        "202609070816-p",
        "---\nproject: [\n---\n\n# Broken\n",
    );
    let notes = query::list(v.root(), None, None).expect("list");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].id, "202609070816-p");
    assert_eq!(notes[0].path, path);
    assert_eq!(notes[0].title, "Broken");
    assert!(notes[0].fields.is_none());
}

// --- show ------------------------------------------------------------------

#[test]
fn show_gives_a_spoke_its_project_and_the_projects_spokes() {
    let v = Vault::new();
    seed(&v);
    let a = query::show(v.root(), "202609070816-write-enforcement")
        .expect("show")
        .expect("found");
    assert_eq!(a.note.kind, Kind::Plan);
    assert_eq!(a.project_id, "202609061846-mnemex");
    assert_eq!(a.project.as_ref().expect("project").title, "mnemex");
    let plan_ids: Vec<&str> = a.spokes.plans.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(plan_ids, ["202609070816-write-enforcement"]);
    let adr_ids: Vec<&str> = a.spokes.adrs.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(adr_ids, ["202609061400-ownership"]);
    assert!(a.spokes.memories.is_empty());
    assert!(a.spokes.contexts.is_empty());
}

#[test]
fn show_on_a_project_is_itself_as_project() {
    let v = Vault::new();
    seed(&v);
    let a = query::show(v.root(), "202609061846-mnemex")
        .expect("show")
        .expect("found");
    assert_eq!(a.project_id, "202609061846-mnemex");
    assert_eq!(a.project.as_ref().expect("project").kind, Kind::Project);
    assert_eq!(a.spokes.plans.len(), 1);
    assert_eq!(a.spokes.adrs.len(), 1);
}

#[test]
fn show_reports_a_dangling_project_link() {
    let v = Vault::new();
    v.note(
        "plans",
        "202609070816-x",
        "project: \"[[202609079999-gone]]\"\nstatus: open\n",
    );
    let a = query::show(v.root(), "202609070816-x")
        .expect("show")
        .expect("found");
    assert_eq!(a.project_id, "202609079999-gone");
    assert!(a.project.is_none());
    assert!(a.spokes.plans.is_empty() && a.spokes.adrs.is_empty());
}

#[test]
fn show_names_an_unparseable_note_rather_than_denying_it() {
    let v = Vault::new();
    let path = v.write(
        "plans",
        "202609070816-p",
        "---\nproject: [\n---\n\n# Broken\n",
    );
    let e = query::show(v.root(), "202609070816-p").expect_err("unparseable");
    assert_eq!(
        e.to_string(),
        format!(
            "`202609070816-p` is a note at {p}, but its frontmatter does not parse, so its links cannot be shown; `mnemex check --path {p}` says why",
            p = path.display()
        )
    );
}

#[test]
fn show_finds_nothing_for_an_unknown_id() {
    let v = Vault::new();
    seed(&v);
    assert_eq!(
        query::show(v.root(), "202609079999-gone").expect("query"),
        None
    );
}

// --- project --for ---------------------------------------------------------

#[test]
fn a_path_match_wins_and_reports_the_stored_path() {
    let v = Vault::new();
    seed(&v);
    let deep = v.dir("work/src/deep");
    let a = query::project(v.root(), &deep, None).expect("query");
    assert_eq!(a.via, Some("path"));
    assert_eq!(a.detail.as_deref(), Some("work"));
    assert_eq!(a.note.as_ref().expect("note").id, "202609061846-mnemex");
}

#[test]
fn a_repo_match_walks_up_to_the_checkout() {
    let v = Vault::new();
    v.checkout("repo", "git@github.com:owner/mnemex.git");
    v.note(
        "projects",
        "202609061846-p",
        "status: active\nrepo: github.com/owner/mnemex\n",
    );
    let deep = v.dir("repo/src/deep");
    let a = query::project(v.root(), &deep, None).expect("query");
    assert_eq!(a.via, Some("repo"));
    assert_eq!(a.detail.as_deref(), Some("github.com/owner/mnemex"));
    assert_eq!(a.note.as_ref().expect("note").id, "202609061846-p");
}

#[test]
fn a_path_match_beats_a_repo_match() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:owner/mnemex.git");
    v.note(
        "projects",
        "202609061846-path",
        "status: active\npath: work\n",
    );
    v.note(
        "projects",
        "202609061847-repo",
        "status: active\nrepo: github.com/owner/mnemex\n",
    );
    let deep = v.dir("work/src");
    let a = query::project(v.root(), &deep, None).expect("query");
    assert_eq!(a.via, Some("path"));
    assert_eq!(a.note.as_ref().expect("note").id, "202609061846-path");
}

#[test]
fn two_projects_with_the_same_repo_are_ambiguous() {
    let v = Vault::new();
    v.checkout("work", "git@github.com:owner/mnemex.git");
    v.note(
        "projects",
        "202609061846-a",
        "status: active\nrepo: github.com/owner/mnemex\n",
    );
    v.note(
        "projects",
        "202609061847-b",
        "status: active\nrepo: github.com/owner/mnemex\n",
    );
    let deep = v.dir("work/src");
    let a = query::project(v.root(), &deep, None).expect("query");
    assert!(a.note.is_none());
    assert_eq!(
        a.candidates
            .iter()
            .map(|n| n.id.as_str())
            .collect::<Vec<_>>(),
        ["202609061846-a", "202609061847-b"]
    );
}

#[test]
fn a_worktree_repo_match_reads_the_commondir() {
    let v = Vault::new();
    v.checkout("main", "git@github.com:owner/mnemex.git");
    let wt = v.dir("wt");
    std::fs::write(wt.join(".git"), "gitdir: ../wt-gitdir\n").expect("write");
    let wt_gitdir = v.dir("wt-gitdir");
    std::fs::write(wt_gitdir.join("commondir"), "../main/.git\n").expect("write");
    v.note(
        "projects",
        "202609061846-p",
        "status: active\nrepo: github.com/owner/mnemex\n",
    );
    let a = query::project(v.root(), &wt, None).expect("query");
    assert_eq!(a.via, Some("repo"));
    assert_eq!(a.note.expect("note").id, "202609061846-p");
}

#[test]
fn a_directory_no_project_owns_says_so() {
    let v = Vault::new();
    seed(&v);
    let lonely = v.dir("lonely");
    let a = query::project(v.root(), &lonely, None).expect("query");
    assert!(a.note.is_none());
    assert!(a.candidates.is_empty());
    assert!(
        a.issue
            .as_deref()
            .is_some_and(|i| i.starts_with("no project owns")),
        "{:?}",
        a.issue
    );
}
