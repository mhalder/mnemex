//! `mnemex brief` — the one command a session runs at start.
//!
//! It reads a project, its spokes, and each note's last-change date, and prints
//! the project body plus the open work, under an output budget. The one `git`
//! spawn in the tool happens here: a single `git log` over the whole vault, used
//! only to rank notes by recency.

use core::fmt::Write as _;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::index::Index;
use crate::kind::Kind;
use crate::markdown::{self, Fences};
use crate::query::{NoteObject, UNTITLED};

/// How many bytes of output `brief` will produce.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Budget {
    /// The limit.
    pub limit: usize,
    /// How many were used.
    pub used: usize,
}

/// Step counts of a plan body.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Steps {
    /// Lines matching `^\s*- \[ \]`.
    pub open: usize,
    /// Open plus lines matching `^\s*- \[[xX]\]`.
    pub total: usize,
}

/// One spoke of the project, read for the brief.
#[derive(Clone, Debug)]
pub struct Spoke {
    /// The note.
    pub note: NoteObject,
    /// Its last-change date, `YYYY-MM-DD`.
    pub changed: String,
    /// Step counts, for a plan.
    pub steps: Option<Steps>,
    /// Its body below the heading, for a context.
    pub body: String,
    /// Whether the context's body fit under the budget.
    pub body_included: bool,
}

/// What `brief` found.
#[derive(Clone, Debug)]
pub struct Brief {
    /// The owning project.
    pub project: NoteObject,
    /// The project's body below its heading.
    pub project_body: String,
    /// Last-change date of every spoke, by id.
    pub changed: BTreeMap<String, String>,
    /// Open plans, in id order.
    pub plans: Vec<Spoke>,
    /// Memories, newest first.
    pub memories: Vec<Spoke>,
    /// ADRs: accepted, then proposed, then deprecated; id order within.
    pub adrs: Vec<Spoke>,
    /// Contexts, newest first.
    pub contexts: Vec<Spoke>,
    /// The `Next` block.
    pub next: Vec<String>,
    /// The budget and what was used.
    pub budget: Budget,
    /// The rendered human output.
    pub human: String,
}

/// The `brief` output budget default, in bytes.
pub const DEFAULT_BUDGET: usize = 16384;

/// Build a brief for the project that owns `dir`.
///
/// # Errors
/// Reports a vault that cannot be read, or a directory no project owns.
pub fn run(root: &Path, dir: &Path, home: Option<&Path>, budget: usize) -> Result<Brief> {
    let answer = crate::query::project(root, dir, home)?;
    let Some(project) = answer.note else {
        let message = answer
            .issue
            .or_else(|| {
                (!answer.candidates.is_empty()).then(|| {
                    let ids = answer
                        .candidates
                        .iter()
                        .map(|n| n.id.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("more than one project owns `{}`: {ids}", dir.display())
                })
            })
            .unwrap_or_else(|| format!("no project owns `{}`", dir.display()));
        return Err(Error::NoProjectForBrief(message));
    };
    build(root, project, budget)
}

/// Build the brief from an already-resolved project.
fn build(root: &Path, project: NoteObject, budget: usize) -> Result<Brief> {
    let index = Index::build(root)?;
    let mut spokes: Vec<Spoke> = index
        .spokes_of(&project.id)
        .into_iter()
        .map(|n| {
            let (title, fields, body) = read_spoke(&n.path, n.kind);
            Spoke {
                note: NoteObject {
                    id: n.id.clone(),
                    kind: n.kind,
                    path: n.path.clone(),
                    title,
                    fields,
                },
                changed: String::new(),
                steps: (n.kind == Kind::Plan).then(|| count_steps(&body)),
                body,
                body_included: false,
            }
        })
        .collect();

    let project_body = read_body(&project.path);
    let changed_dates = last_change_dates(root)?;
    for spoke in &mut spokes {
        spoke.changed = changed_dates
            .get(&spoke.note.path)
            .cloned()
            .unwrap_or_default();
    }

    let mut plans: Vec<Spoke> = spokes
        .iter()
        .filter(|s| s.note.kind == Kind::Plan && status_of(s) != Some("done"))
        .cloned()
        .collect();
    plans.sort_by(|a, b| a.note.id.cmp(&b.note.id));

    let mut memories: Vec<Spoke> = spokes
        .iter()
        .filter(|s| s.note.kind == Kind::Memory)
        .cloned()
        .collect();
    sort_newest(&mut memories);

    let mut adrs: Vec<Spoke> = spokes
        .iter()
        .filter(|s| s.note.kind == Kind::Adr)
        .cloned()
        .collect();
    adrs.sort_by(|a, b| {
        adr_rank(status_of(a))
            .cmp(&adr_rank(status_of(b)))
            .then_with(|| a.note.id.cmp(&b.note.id))
    });

    let mut contexts: Vec<Spoke> = spokes
        .iter()
        .filter(|s| s.note.kind == Kind::Context)
        .cloned()
        .collect();
    sort_newest(&mut contexts);

    let next = next_lines(&plans);

    let changed: BTreeMap<String, String> = spokes
        .iter()
        .map(|s| (s.note.id.clone(), s.changed.clone()))
        .collect();

    // Decide which context bodies fit, newest first, then render once.
    let prefix = prefix_text(&project, &project_body, &plans, &memories, &adrs);
    let mut used = prefix.len();
    let mut first_omitted = false;
    for c in &mut contexts {
        let section = context_section(c);
        if !first_omitted && used + section.len() <= budget {
            used += section.len();
            c.body_included = true;
        } else {
            c.body_included = false;
            first_omitted = true;
        }
    }
    let human = render_tail(prefix, &contexts, &next);

    Ok(Brief {
        project,
        project_body,
        changed,
        plans,
        memories,
        adrs,
        contexts,
        next,
        budget: Budget {
            limit: budget,
            used: human.len(),
        },
        human,
    })
}

/// Read a spoke's title, fields and body below its heading.
fn read_spoke(path: &Path, kind: Kind) -> (String, Option<Vec<crate::frontmatter::Field>>, String) {
    let Ok(src) = std::fs::read_to_string(path) else {
        return (UNTITLED.to_owned(), None, String::new());
    };
    let body = crate::frontmatter::body_after_block(&src);
    let title = markdown::first_heading(body.lines()).unwrap_or_else(|| UNTITLED.to_owned());
    let fields = crate::frontmatter::parse(&src)
        .ok()
        .map(|doc| crate::query::canonical_fields(kind, &doc));
    (title, fields, body_below_heading(body))
}

/// The body of a note below its first `# ` heading, verbatim otherwise.
fn read_body(path: &Path) -> String {
    let Ok(src) = std::fs::read_to_string(path) else {
        return String::new();
    };
    body_below_heading(crate::frontmatter::body_after_block(&src))
}

/// Drop the first `# ` heading line of a body, so a project's body is shown
/// without restating its title.
fn body_below_heading(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut fences = Fences::default();
    let mut removed = false;
    for line in body.split_inclusive('\n') {
        if fences.is_prose(line) && !removed && markdown::heading(line).is_some() {
            removed = true;
            continue;
        }
        out.push_str(line);
    }
    out
}

/// Count open and total step lines of a plan body.
fn count_steps(body: &str) -> Steps {
    let mut steps = Steps::default();
    for line in body.lines() {
        let line = line.trim_start();
        if line.starts_with("- [ ]") {
            steps.open += 1;
            steps.total += 1;
        } else if line.starts_with("- [x]") || line.starts_with("- [X]") {
            steps.total += 1;
        }
    }
    steps
}

/// A note's `status` scalar.
fn status_of(spoke: &Spoke) -> Option<&str> {
    spoke
        .note
        .fields
        .as_ref()?
        .iter()
        .find(|f| f.key == "status")
        .and_then(|f| f.value.as_scalar())
}

/// The ADR ordering: accepted first, then proposed, deprecated last.
fn adr_rank(status: Option<&str>) -> u8 {
    match status {
        Some("accepted") => 0,
        Some("deprecated") => 2,
        _ => 1,
    }
}

/// Sort newest first by `changed`, ties broken by id.
fn sort_newest(spokes: &mut [Spoke]) {
    spokes.sort_by(|a, b| {
        b.changed
            .cmp(&a.changed)
            .then_with(|| a.note.id.cmp(&b.note.id))
    });
}

/// The `Next` block: which plans to read, and which to close.
fn next_lines(plans: &[Spoke]) -> Vec<String> {
    let mut out = vec![];
    let mut stale = 0usize;
    for plan in plans {
        let open = plan.steps.map_or(0, |s| s.open);
        if open > 0 {
            out.push(format!(
                "read plans/{}.md before starting: {open} open steps",
                plan.note.id
            ));
        } else {
            stale += 1;
        }
    }
    if stale > 0 {
        out.push(if stale == 1 {
            "1 plan is done and still listed as open steps count 0: set it done".to_owned()
        } else {
            format!("{stale} plans are done and still listed as open steps count 0: set them done")
        });
    }
    out
}

/// The sections that are always emitted, before the budget cuts in: the
/// header, the project body, open plans, memories, and ADRs.
fn prefix_text(
    project: &NoteObject,
    project_body: &str,
    plans: &[Spoke],
    memories: &[Spoke],
    adrs: &[Spoke],
) -> String {
    let path = project
        .fields
        .as_ref()
        .and_then(|fs| fs.iter().find(|f| f.key == "path"))
        .and_then(|f| f.value.as_scalar())
        .unwrap_or("no path");
    let repo = project
        .fields
        .as_ref()
        .and_then(|fs| fs.iter().find(|f| f.key == "repo"))
        .and_then(|f| f.value.as_scalar())
        .unwrap_or("no repo");

    let mut out = String::new();
    let _ = writeln!(out, "# mnemex project: {}", project.title);
    let _ = writeln!(out, "{} · {} · {}", project.id, path, repo);
    out.push('\n');
    out.push_str(project_body);
    if !project_body.ends_with('\n') {
        out.push('\n');
    }

    out.push_str("\n## Open plans\n");
    for plan in plans {
        let steps = plan.steps.unwrap_or_default();
        let mut line = format!(
            "- {} — {} ({} of {} steps open)",
            plan.note.id, plan.note.title, steps.open, steps.total
        );
        for r in refs_of(&plan.note) {
            let _ = write!(line, " · {r}");
        }
        let _ = writeln!(out, "{line}");
    }

    out.push_str("\n## Memories\n");
    for m in memories {
        let _ = writeln!(out, "- {} {} — {}", m.changed, m.note.id, m.note.title);
    }

    out.push_str("\n## ADRs\n");
    for a in adrs {
        let status = status_of(a).unwrap_or("");
        let _ = writeln!(out, "- {} — {} ({status})", a.note.id, a.note.title);
    }

    out
}

/// Append the contexts list, the fitting context bodies, and `Next` to a
/// rendered prefix.
fn render_tail(mut out: String, contexts: &[Spoke], next: &[String]) -> String {
    out.push_str("\n## Contexts\n");
    for c in contexts {
        let suffix = if c.body_included {
            ""
        } else {
            " (body omitted, over budget)"
        };
        let _ = writeln!(out, "- {} — {}{}", c.note.id, c.note.title, suffix);
    }
    for c in contexts {
        if c.body_included {
            out.push_str(&context_section(c));
        }
    }
    if !next.is_empty() {
        out.push_str("\n## Next\n");
        for line in next {
            let _ = writeln!(out, "- {line}");
        }
    }
    out
}

/// A context's body section, as it would be appended.
fn context_section(c: &Spoke) -> String {
    let mut out = format!("\n## Context: {}\n", c.note.title);
    out.push_str(&c.body);
    if !c.body.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// A note's `refs` entries, in order.
fn refs_of(note: &NoteObject) -> Vec<&str> {
    note.fields
        .as_ref()
        .and_then(|fs| fs.iter().find(|f| f.key == "refs"))
        .and_then(|f| f.value.as_list())
        .map(|items| items.iter().map(String::as_str).collect())
        .unwrap_or_default()
}

/// The last-change date of every governed note: from one `git log` when the
/// vault is a git repository, else file mtime.
fn last_change_dates(root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let git = if root.join(".git").exists() {
        git_dates(root)
    } else {
        None
    };
    let mut out = BTreeMap::new();
    for file in crate::vault::governed_files(root)? {
        let date = git
            .as_ref()
            .and_then(|map| map.get(&file.path).cloned())
            .or_else(|| mtime_date(&file.path));
        out.insert(file.path, date.unwrap_or_default());
    }
    Ok(out)
}

/// Run `git log` once over the vault, mapping each path to its latest commit
/// date.
fn git_dates(root: &Path) -> Option<BTreeMap<PathBuf, String>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["log", "--format=%ad", "--date=short", "--name-only"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut map = BTreeMap::new();
    let mut current = String::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if is_short_date(line) {
            current.clear();
            current.push_str(line);
        } else {
            let path = root.join(line);
            map.entry(path).or_insert_with(|| current.clone());
        }
    }
    Some(map)
}

/// Whether `s` is `YYYY-MM-DD`.
fn is_short_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// A file's modification time as `YYYY-MM-DD`, in local time.
fn mtime_date(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let timestamp = jiff::Timestamp::try_from(modified).ok()?;
    let zoned = timestamp.to_zoned(jiff::tz::TimeZone::system());
    Some(zoned.strftime("%Y-%m-%d").to_string())
}

// --- JSON ---------------------------------------------------------------

#[derive(Serialize)]
struct StepsJson {
    open: usize,
    total: usize,
}

#[derive(Serialize)]
struct PlanJson<'a> {
    #[serde(flatten)]
    note: crate::report::NoteJson<'a>,
    steps: StepsJson,
}

#[derive(Serialize)]
struct ContextJson<'a> {
    #[serde(flatten)]
    note: crate::report::NoteJson<'a>,
    body_included: bool,
}

#[derive(Serialize)]
struct BriefJson<'a> {
    version: u32,
    project: crate::report::NoteJson<'a>,
    changed: BTreeMap<String, String>,
    plans: Vec<PlanJson<'a>>,
    memories: Vec<crate::report::NoteJson<'a>>,
    adrs: Vec<crate::report::NoteJson<'a>>,
    contexts: Vec<ContextJson<'a>>,
    next: Vec<String>,
    budget: Budget,
}

/// The `brief --json` envelope.
#[must_use]
pub fn json(brief: &Brief) -> String {
    crate::report::envelope(&BriefJson {
        version: crate::report::VERSION,
        project: (&brief.project).into(),
        changed: brief.changed.clone(),
        plans: brief
            .plans
            .iter()
            .map(|s| PlanJson {
                note: (&s.note).into(),
                steps: StepsJson {
                    open: s.steps.unwrap_or_default().open,
                    total: s.steps.unwrap_or_default().total,
                },
            })
            .collect(),
        memories: brief.memories.iter().map(|s| (&s.note).into()).collect(),
        adrs: brief.adrs.iter().map(|s| (&s.note).into()).collect(),
        contexts: brief
            .contexts
            .iter()
            .map(|s| ContextJson {
                note: (&s.note).into(),
                body_included: s.body_included,
            })
            .collect(),
        next: brief.next.clone(),
        budget: brief.budget,
    })
}
