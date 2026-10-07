//! The `git remote` shape: canonicalisation and where a value comes from.
//!
//! Canonicalisation is what makes `repo` an *identity* rather than a string: an
//! ssh clone and an https clone of one repository must compare equal, or the
//! drift rule would fire on two spellings of the same truth.

mod support;

use mnemex::git;
use support::Vault;

/// Credentials can only come before the host: an `@` in the path is part of
/// the path, and a path segment holding one is refused, never read as a host.
#[test]
fn an_at_sign_in_the_path_never_becomes_the_host() {
    for url in [
        "https://example.com/team@x.org/repo",
        "https://user@example.com/team@x.org/repo.git",
        "ssh://git@example.com/a@b.com/repo",
    ] {
        assert_eq!(git::canonical(url), None, "{url}");
    }
    assert_eq!(
        git::canonical("https://user@example.com/team/repo").as_deref(),
        Some("example.com/team/repo"),
        "credentials before the host are still dropped"
    );
}

/// Every spelling of one repository canonicalises to one value.
#[test]
fn every_spelling_of_one_repository_agrees() {
    for url in [
        "git@github.com:owner/mnemex.git",
        "git@github.com:owner/mnemex",
        "ssh://git@github.com/owner/mnemex.git",
        "ssh://git@github.com:2222/owner/mnemex.git",
        "https://github.com/owner/mnemex.git",
        "https://github.com/owner/mnemex",
        "https://owner@github.com/owner/mnemex.git",
        "http://github.com/owner/mnemex/",
        "git://github.com/owner/mnemex.git",
        "  https://GitHub.com/owner/mnemex.git  ",
        "github.com/owner/mnemex",
    ] {
        assert_eq!(
            git::canonical(url).as_deref(),
            Some("github.com/owner/mnemex"),
            "{url}"
        );
    }
}

/// The stored form is the fixed point: checking a value is canonicalising it
/// again and finding it unchanged.
#[test]
fn canonicalisation_is_idempotent() {
    for url in [
        "git@git.sr.ht:~owner/mnemex.git",
        "https://gitlab.com/group/subgroup/name.git",
        "ssh://git@localhost:2222/srv/name.git",
        "https://example.com/owner/name.git",
    ] {
        let once = git::canonical(url).expect("a usable remote");
        assert_eq!(git::canonical(&once).as_deref(), Some(once.as_str()));
        assert!(
            git::is_canonical(&once),
            "{once} is not its own fixed point"
        );
    }
}

/// The host is identity, so it is lowercased; the path is not, because a
/// repository name is not case-insensitive.
#[test]
fn the_host_is_lowercased_and_the_path_is_not() {
    assert_eq!(
        git::canonical("git@GitHub.COM:Owner/Mnemex.git").as_deref(),
        Some("github.com/Owner/Mnemex")
    );
}

/// A local checkout is what `path` names; it is not an identity, and is refused
/// rather than stored as one.
#[test]
fn a_remote_with_no_host_is_refused() {
    for url in [
        "file:///srv/git/name.git",
        "/srv/git/name.git",
        "./name",
        "~/src/name",
        "../name",
        "name",
        "owner/name",
        "",
        "   ",
        "git@github.com:",
        "https://github.com/",
        "https://github.com",
        "git@github.com:owner/name .git",
        "https://github.com/owner/../name",
    ] {
        assert_eq!(git::canonical(url), None, "{url} was accepted");
        assert!(!git::is_canonical(url), "{url} passed as canonical");
    }
}

/// The stored form is `host/owner/name`, so a remote naming only one of the two
/// is no repository this tool can identify.
#[test]
fn a_remote_needs_owner_and_name() {
    for url in [
        "https://github.com/owner",
        "git@github.com:name.git",
        "github.com/name",
    ] {
        assert_eq!(git::canonical(url), None, "{url} was accepted");
    }
}

/// The refusal holds up an example; the example must be a remote this tool
/// accepts.
#[test]
fn the_refusal_example_is_accepted() {
    let refusal = mnemex::error::Error::NotARemote("x".to_owned()).to_string();
    let example = refusal.split('`').nth(3).expect("an example");
    assert_eq!(
        git::canonical(example).as_deref(),
        Some("github.com/owner/name"),
        "{refusal}"
    );
}

/// A canonical value is not one that merely round-trips: the spellings the
/// verbs accept are not the spelling the block holds.
#[test]
fn an_uncanonical_spelling_is_not_canonical() {
    for url in [
        "https://github.com/owner/name.git",
        "git@github.com:owner/name.git",
        "GitHub.com/owner/name",
        "github.com/owner/name/",
        "github.com/owner/name.git",
    ] {
        assert!(git::canonical(url).is_some(), "{url} should canonicalise");
        assert!(!git::is_canonical(url), "{url} is not the stored form");
    }
}

/// `origin` is the remote a project's directory points at. A directory that is
/// no checkout has none, which is silence rather than a finding.
#[test]
fn origin_is_read_from_the_config_of_a_plain_checkout() {
    let v = Vault::bare();
    let work = v.dir("work");
    assert_eq!(git::origin(&work), None);

    let git_dir = v.dir("work/.git");
    std::fs::write(
        git_dir.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\
         [remote \"upstream\"]\n\turl = https://github.com/upstream/name.git\n\
         [remote \"origin\"]\n\turl = git@github.com:owner/name.git\n\tfetch = +refs/heads/*\n",
    )
    .expect("config");

    assert_eq!(git::origin(&work).as_deref(), Some("github.com/owner/name"));
}

/// A worktree's `.git` is a file naming a git directory, and that directory
/// borrows the main checkout's config through `commondir`.
#[test]
fn origin_follows_a_git_file_and_its_commondir() {
    let v = Vault::bare();
    let main = v.dir("main/.git");
    std::fs::write(
        main.join("config"),
        "[remote \"origin\"]\n\turl = https://github.com/owner/name.git\n",
    )
    .expect("config");
    let linked = v.dir("main/.git/worktrees/feature");
    std::fs::write(linked.join("commondir"), "../..\n").expect("commondir");

    let work = v.dir("work");
    std::fs::write(
        work.join(".git"),
        "gitdir: ../main/.git/worktrees/feature\n",
    )
    .expect("gitdir file");

    assert_eq!(git::origin(&work).as_deref(), Some("github.com/owner/name"));
}

/// A checkout whose origin is a local path has no identity to store, and a
/// config with no origin at all has nothing to say.
#[test]
fn an_origin_with_no_identity_is_silence() {
    let v = Vault::bare();
    let work = v.dir("work");
    let git_dir = v.dir("work/.git");

    std::fs::write(
        git_dir.join("config"),
        "[remote \"origin\"]\n\turl = /srv/git/name.git\n",
    )
    .expect("config");
    assert_eq!(git::origin(&work), None);

    std::fs::write(git_dir.join("config"), "[core]\n\tbare = false\n").expect("config");
    assert_eq!(git::origin(&work), None);

    std::fs::write(
        git_dir.join("config"),
        "# [remote \"origin\"]\n; url = git@github.com:o/n.git\n",
    )
    .expect("config");
    assert_eq!(git::origin(&work), None);
}

/// Every remote is read, in config order, and one that is no identity — a local
/// path, a bare word — or a repeat of one already read is left out rather than
/// counted.
#[test]
fn remotes_are_read_from_the_config_and_canonicalised() {
    let v = Vault::bare();
    let work = v.dir("work");
    assert_eq!(git::remotes(&work), Vec::<String>::new());

    let git_dir = v.dir("work/.git");
    std::fs::write(
        git_dir.join("config"),
        "[remote \"origin\"]\n\turl = https://github.com/upstream/name.git\n\n\
         [branch \"main\"]\n\tremote = origin\n\n\
         [remote \"private\"]\n\turl = git@git.sr.ht:~owner/name\n\n\
         [remote \"local\"]\n\turl = /srv/git/name.git\n\n\
         [remote \"same\"]\n\turl = git@github.com:upstream/name.git\n",
    )
    .expect("config");

    assert_eq!(
        git::remotes(&work),
        vec![
            "github.com/upstream/name".to_owned(),
            "git.sr.ht/~owner/name".to_owned(),
        ]
    );
    assert_eq!(
        git::origin(&work).as_deref(),
        Some("github.com/upstream/name")
    );
}
