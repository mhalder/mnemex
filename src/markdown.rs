//! The little Markdown the tool reads in a body: where a fenced code block opens
//! and closes, and which line is a note's title.
//!
//! A `# ` line inside a fence is code, not a heading, so every reader of a title
//! and the one writer of a heading walk the fences the same way.

/// Whether each line of a body, fed in order, is inside a fenced code block.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fences {
    /// The character and run length of the fence that is open, if one is.
    open: Option<(char, usize)>,
}

impl Fences {
    /// Feed the next line, and say whether it is prose: outside every fence and
    /// not itself a fence marker.
    pub fn is_prose(&mut self, line: &str) -> bool {
        let Some((c, run)) = fence_marker(line) else {
            return self.open.is_none();
        };
        self.open = match self.open {
            None => Some((c, run)),
            // Only the same character, at least as long, closes a fence.
            Some((open, len)) if open == c && run >= len => None,
            still_open => still_open,
        };
        false
    }
}

/// The character and run length of a line that opens or closes a code fence:
/// three or more backticks or tildes, indented by at most three spaces.
fn fence_marker(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let c = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let run = trimmed.chars().take_while(|x| *x == c).count();
    (run >= 3).then_some((c, run))
}

/// The text of a title heading: a line that starts with `# `, trimmed.
#[must_use]
pub fn heading(line: &str) -> Option<&str> {
    line.strip_prefix("# ").map(str::trim)
}

/// The first title heading among `lines` that is outside every fence.
pub fn first_heading<I, S>(lines: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut fences = Fences::default();
    lines.into_iter().find_map(|line| {
        let line = line.as_ref();
        fences
            .is_prose(line)
            .then(|| heading(line))
            .flatten()
            .map(ToOwned::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fence_closes_only_on_its_own_character_at_least_as_long() {
        let body = "````\n# code\n```\n~~~~\n# still code\n````\n# Title\n";
        assert_eq!(first_heading(body.lines()).as_deref(), Some("Title"));
    }

    #[test]
    fn an_indented_line_is_neither_a_fence_nor_a_heading() {
        let body = "    ```\n  # not a title\n# Title\n";
        assert_eq!(first_heading(body.lines()).as_deref(), Some("Title"));
    }
}
