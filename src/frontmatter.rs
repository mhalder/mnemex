//! The frontmatter engine.
//!
//! A note is `---\n<yaml block>\n---\n<body>`. The block is a **flat** mapping:
//! every value is a scalar or a list of scalars. The body is bytes and passes
//! through untouched.
//!
//! Two things a YAML mapping will not give you are compensated here:
//! duplicate keys are recorded from the parser's events as the mapping is
//! built, because the mapping keeps only the last occurrence; and an empty
//! value's span points past the value, so diagnostics about one anchor to its
//! key instead.

use core::fmt::Write as _;
use std::collections::HashSet;

use saphyr::{MarkedYaml, Scalar, ScalarStyle, YamlData, YamlLoader};
use saphyr_parser::{Event, Parser, Span, SpannedEventReceiver};

use crate::kind::Kind;

/// A one-based `(line, column)` in whole-file coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    /// One-based line.
    pub line: usize,
    /// One-based column.
    pub column: usize,
}

impl Position {
    /// The position `line:column`.
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A frontmatter value the flat model can hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A scalar, as its frontmatter text.
    Scalar(String),
    /// A list of scalars.
    List(Vec<String>),
    /// Neither a scalar nor a list of scalars: a nested mapping, or a list
    /// holding a non-scalar. Recorded rather than silently flattened.
    Unsupported,
}

impl Value {
    /// The scalar text, if this is a scalar.
    #[must_use]
    pub fn as_scalar(&self) -> Option<&str> {
        match self {
            Value::Scalar(s) => Some(s),
            _ => None,
        }
    }

    /// The items, if this is a list.
    #[must_use]
    pub fn as_list(&self) -> Option<&[String]> {
        match self {
            Value::List(v) => Some(v),
            _ => None,
        }
    }

    /// Whether this is an empty scalar — the only shape that counts as an
    /// absent value.
    #[must_use]
    pub fn is_empty_scalar(&self) -> bool {
        matches!(self, Value::Scalar(s) if s.is_empty())
    }
}

/// One key/value pair of the block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The key.
    pub key: String,
    /// Where the key starts.
    pub key_at: Position,
    /// The value.
    pub value: Value,
    /// Where the value starts.
    pub value_at: Position,
    /// Where each list item starts. Empty for scalars.
    pub items_at: Vec<Position>,
}

impl Field {
    /// Where a diagnostic about this field anchors: the value, except when the
    /// value is an empty scalar — there is nothing to underline — in which case
    /// the key.
    #[must_use]
    pub fn anchor(&self) -> Position {
        if self.value.is_empty_scalar() {
            self.key_at
        } else {
            self.value_at
        }
    }

    /// Where a diagnostic about list item `i` anchors.
    #[must_use]
    pub fn item_anchor(&self, i: usize) -> Position {
        self.items_at
            .get(i)
            .copied()
            .unwrap_or_else(|| self.anchor())
    }
}

/// A parsed note.
#[derive(Clone, Debug)]
pub struct Document {
    /// The fields, in the order they were read.
    pub fields: Vec<Field>,
    /// Keys the block states more than once, with where each repeat starts.
    pub duplicates: Vec<(String, Position)>,
    /// Everything after the closing fence, verbatim.
    pub body: String,
}

impl Document {
    /// The field named `key`, if the block has one.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// The mutable field named `key`, if the block has one.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Field> {
        self.fields.iter_mut().find(|f| f.key == key)
    }

    /// Whether every value fits the flat model and no key is stated twice, and
    /// so whether an authoring verb may rewrite this document at all: a
    /// rewrite would keep only the last of a repeated key.
    #[must_use]
    pub fn is_renderable(&self) -> bool {
        self.duplicates.is_empty() && self.fields.iter().all(|f| f.value != Value::Unsupported)
    }

    /// Remove the field named `key`.
    pub fn remove(&mut self, key: &str) {
        self.fields.retain(|f| f.key != key);
    }

    /// Set `key` to `value`.
    ///
    /// A new field lands in canonical position rather than being appended
    /// because [`Document::render`] emits known fields in schema order, so
    /// where a new field sits in [`Document::fields`] only decides the order of
    /// *unknown* keys, which is the order they were read.
    pub fn set(&mut self, key: &str, value: Value) {
        if let Some(f) = self.get_mut(key) {
            f.value = value;
            return;
        }
        let field = Field {
            key: key.to_owned(),
            key_at: Position::new(1, 1),
            value,
            value_at: Position::new(1, 1),
            items_at: vec![],
        };
        // Canonical position is computed at render time for known fields, so
        // the read order only has to be sane; append is enough for unknowns.
        self.fields.push(field);
    }

    /// Canonical emission for a kind: every known field this
    /// document has, in schema order, then every unknown field in read order.
    #[must_use]
    pub fn render(&self, kind: Kind) -> String {
        let mut out = String::from("---\n");
        let spec = kind.spec();
        for f in spec.fields {
            if let Some(field) = self.get(f.name) {
                emit(&mut out, field);
            }
        }
        for field in &self.fields {
            if spec.field(&field.key).is_none() {
                emit(&mut out, field);
            }
        }
        out.push_str("---\n");
        out.push_str(&self.body);
        out
    }
}

/// Emit one field.
fn emit(out: &mut String, field: &Field) {
    let key = quote(&field.key);
    match &field.value {
        Value::Scalar(s) if s.is_empty() => {
            out.push_str(&key);
            out.push_str(":\n");
        }
        Value::Scalar(s) => {
            out.push_str(&key);
            out.push_str(": ");
            out.push_str(&quote(s));
            out.push('\n');
        }
        Value::List(items) if items.is_empty() => {
            out.push_str(&key);
            out.push_str(": []\n");
        }
        Value::List(items) => {
            out.push_str(&key);
            out.push_str(":\n");
            for item in items {
                out.push_str("  - ");
                out.push_str(&quote(item));
                out.push('\n');
            }
        }
        // An unrenderable document is refused by every writing verb before it
        // reaches here; emitting the key alone at least never invents a value.
        Value::Unsupported => {
            out.push_str(&key);
            out.push_str(":\n");
        }
    }
}

/// Characters that, at the start of a scalar, give it a YAML meaning.
///
/// The first fourteen are the spec's list. `]`, `}` and `?` are
/// added for the same reason control characters are: a plain scalar opening
/// with one of them does not parse back as the scalar it was, and the round
/// trip the spec requires would not hold. A lone `-` is quoted for the same
/// reason: as a mapping value it opens a block sequence.
const LEADING: [char; 17] = [
    '[', '{', '>', '|', '*', '&', '!', '%', '@', '`', '\'', '"', '#', ',', ']', '}', '?',
];

/// Whether a scalar needs double quotes.
///
/// The listed cases are the spec's. Control characters are quoted too: a raw
/// newline in a plain scalar would not survive the round trip the spec requires,
/// and the spec's list does not reach it. A plain scalar YAML would resolve as
/// something other than a string — `007`, `1e3`, `0x10`, `1.50` — is quoted too,
/// so its text survives the round trip instead of its decoded number. A key
/// opening with `--- ` or `... ` would start or end a YAML document at the start
/// of a line, so either opening is quoted too.
#[must_use]
pub fn needs_quotes(s: &str) -> bool {
    s.is_empty()
        || s.starts_with(LEADING)
        || s == "-"
        || s.starts_with("- ")
        || s.starts_with("--- ")
        || s.starts_with("... ")
        || s.starts_with(": ")
        || s.contains(": ")
        || s.ends_with(':')
        || s.contains(" #")
        || s.trim() != s
        || s.chars().any(char::is_control)
        || !matches!(Scalar::parse_from_cow(s.into()), Scalar::String(_))
}

/// Emit a scalar, double-quoted if it needs to be.
#[must_use]
pub fn quote(s: &str) -> String {
    if !needs_quotes(s) {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A frontmatter parse failure. Reported as `MX001`.
#[derive(Clone, Debug, thiserror::Error)]
#[error("{message}")]
pub struct ParseError {
    /// The parser's own message.
    pub message: String,
    /// Where it applies, in whole-file coordinates.
    pub at: Position,
}

impl ParseError {
    fn at_start(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            at: Position::new(1, 1),
        }
    }
}

/// Strip a single trailing carriage return, so a CRLF file's lines compare.
fn line_body(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

/// Split a note into its frontmatter block and its body.
///
/// Returns the block text and the body, or the fence error.
fn split_fences(src: &str) -> Result<(&str, &str), ParseError> {
    // A byte-order mark is an encoding artefact, not text before the fence.
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let after_open = src
        .strip_prefix("---\n")
        .or_else(|| src.strip_prefix("---\r\n"))
        .ok_or_else(|| ParseError::at_start("note does not open with a `---` frontmatter fence"))?;
    let mut offset = 0usize;
    for line in after_open.split_inclusive('\n') {
        let text = line.strip_suffix('\n').unwrap_or(line);
        if line_body(text) == "---" {
            return Ok((&after_open[..offset], &after_open[offset + line.len()..]));
        }
        offset += line.len();
    }
    Err(ParseError::at_start(
        "frontmatter block is never closed by a `---` fence",
    ))
}

/// Everything after a note's leading frontmatter block, whether or not the
/// block parses; the whole text when there is no closed block to drop.
#[must_use]
pub fn body_after_block(src: &str) -> &str {
    split_fences(src).map_or(src, |(_, body)| body)
}

/// The loader, and a record of every scalar key the top-level mapping states.
///
/// The mapping the loader builds keeps only the last occurrence of a key, so
/// the keys are taken from the parser's events on the way in. That makes a key
/// exactly what the parser reads as one — any spelling, quoted or plain, with
/// a quoted key the same key as its plain spelling — rather than what a line
/// pattern guesses.
struct KeyRecorder<'input> {
    loader: YamlLoader<'input, MarkedYaml<'input>>,
    line_offset: usize,
    /// How many collections the next event is nested in.
    depth: usize,
    /// How many nodes have started directly inside the top-level collection:
    /// in a mapping, an even count means the next node is a key.
    entries: usize,
    keys: Vec<(String, Position)>,
}

impl<'input> SpannedEventReceiver<'input> for KeyRecorder<'input> {
    fn on_event(&mut self, ev: Event<'input>, span: Span) {
        let starts_node = matches!(
            ev,
            Event::Scalar(..)
                | Event::Alias(_)
                | Event::MappingStart(..)
                | Event::SequenceStart(..)
        );
        if starts_node && self.depth == 1 {
            if self.entries.is_multiple_of(2)
                && let Event::Scalar(text, ..) = &ev
            {
                let at = Position::new(span.start.line() + self.line_offset, span.start.col() + 1);
                self.keys.push((text.to_string(), at));
            }
            self.entries += 1;
        }
        match ev {
            Event::MappingStart(..) | Event::SequenceStart(..) => self.depth += 1,
            Event::MappingEnd | Event::SequenceEnd => self.depth -= 1,
            _ => {}
        }
        self.loader.on_event(ev, span);
    }
}

/// Every key after its first occurrence, in the order stated.
fn repeats(keys: Vec<(String, Position)>) -> Vec<(String, Position)> {
    let mut seen = HashSet::new();
    keys.into_iter()
        .filter(|(key, _)| !seen.insert(key.clone()))
        .collect()
}

/// Whether a scalar, as written, is YAML's null: plain and spelled `~`,
/// `null`, or empty.
fn is_null(text: &str, style: ScalarStyle) -> bool {
    style == ScalarStyle::Plain && matches!(Scalar::parse_from_cow(text.into()), Scalar::Null)
}

/// A scalar node as its frontmatter text: exactly as written, so `1.50`,
/// `0x10`, and `True` survive a write, except that a null is the empty text.
fn scalar_text(node: &MarkedYaml<'_>) -> Option<String> {
    let YamlData::Representation(text, style, _) = &node.data else {
        return None;
    };
    Some(if is_null(text, *style) {
        String::new()
    } else {
        text.to_string()
    })
}

fn position_of(node: &MarkedYaml<'_>, line_offset: usize) -> Position {
    Position::new(
        node.span.start.line() + line_offset,
        node.span.start.col() + 1,
    )
}

fn read_value(node: &MarkedYaml<'_>, line_offset: usize) -> (Value, Vec<Position>) {
    if let Some(text) = scalar_text(node) {
        return (Value::Scalar(text), vec![]);
    }
    let YamlData::Sequence(items) = &node.data else {
        return (Value::Unsupported, vec![]);
    };
    let mut texts = Vec::with_capacity(items.len());
    let mut spans = Vec::with_capacity(items.len());
    for item in items {
        let Some(text) = scalar_text(item) else {
            return (Value::Unsupported, vec![]);
        };
        texts.push(text);
        spans.push(position_of(item, line_offset));
    }
    (Value::List(texts), spans)
}

/// Parse a note into its frontmatter and its body.
///
/// # Errors
/// Returns [`ParseError`] when the fences or the YAML block are malformed. A
/// parse failure is reported as `MX001` and short-circuits every other rule.
pub fn parse(src: &str) -> Result<Document, ParseError> {
    let (block, body) = split_fences(src)?;
    // The YAML parser sees only the block, so its lines are shifted past the
    // opening fence and its zero-based columns are shifted to one-based.
    let line_offset = 1;

    // Scalars are kept as written rather than decoded, so a write never turns
    // `1.50` into `1.5`; `scalar_text` decides what the written text means.
    let mut recorder = KeyRecorder {
        loader: YamlLoader::default(),
        line_offset,
        depth: 0,
        entries: 0,
        keys: vec![],
    };
    recorder.loader.early_parse(false);
    Parser::new_from_str(block)
        .load(&mut recorder, true)
        .map_err(|e| ParseError {
            message: e.info().to_owned(),
            at: Position::new(e.marker().line() + line_offset, e.marker().col() + 1),
        })?;
    let duplicates = repeats(recorder.keys);
    let docs = recorder.loader.into_documents();
    // Only the first document is read, so a write would silently drop the
    // rest.
    if let Some(second) = docs.get(1) {
        return Err(ParseError {
            message: "frontmatter holds a second YAML document after `...`; everything after it would be lost, so move those fields above it".to_owned(),
            at: position_of(second, line_offset),
        });
    }

    let Some(root) = docs.first() else {
        return Ok(Document {
            fields: vec![],
            duplicates,
            body: body.to_owned(),
        });
    };

    let mapping = match &root.data {
        YamlData::Mapping(m) => m,
        YamlData::Representation(text, style, _) if is_null(text, *style) => {
            return Ok(Document {
                fields: vec![],
                duplicates,
                body: body.to_owned(),
            });
        }
        _ => {
            return Err(ParseError {
                message: "frontmatter must be a mapping".to_owned(),
                at: position_of(root, line_offset),
            });
        }
    };

    let mut fields = Vec::with_capacity(mapping.len());
    for (k, v) in mapping {
        let key = match &k.data {
            YamlData::Representation(text, style, _)
                if *style != ScalarStyle::Plain
                    || matches!(Scalar::parse_from_cow(text.clone()), Scalar::String(_)) =>
            {
                text
            }
            // A plain scalar YAML reads as a number, a bool or a null is still a
            // key once quoted, so the message says how.
            YamlData::Representation(text, ..) => {
                return Err(ParseError {
                    message: format!(
                        "frontmatter keys must be strings; quote `{text}` as `\"{text}\"`"
                    ),
                    at: position_of(k, line_offset),
                });
            }
            _ => {
                return Err(ParseError {
                    message: "frontmatter keys must be strings".to_owned(),
                    at: position_of(k, line_offset),
                });
            }
        };
        let (value, items_at) = read_value(v, line_offset);
        fields.push(Field {
            key: key.to_string(),
            key_at: position_of(k, line_offset),
            value,
            value_at: position_of(v, line_offset),
            items_at,
        });
    }
    Ok(Document {
        fields,
        duplicates,
        body: body.to_owned(),
    })
}
