//! A small, dependency-free Markdown reader for album liner notes.
//!
//! Liner notes are user prose, not a web page. This parser therefore covers the
//! subset that belongs in notes — headings, paragraphs, emphasis, inline code,
//! links, quotes, lists, fenced code and rules — and deliberately produces
//! structured blocks instead of HTML. The UI paints those blocks with its own
//! fonts, and [`plain_text`] gives screen readers and search a flat form.
//!
//! Raw HTML is never interpreted. A link is only kept when it uses `http`,
//! `https` or `mailto`; anything else stays visible as ordinary text, so a
//! `javascript:` payload in a shared notes file cannot become a button.

use serde::{Deserialize, Serialize};

/// Notes larger than this are refused. It is roomy for an essay (about 250
/// pages of plain text) while keeping an accidental paste from stalling the UI.
pub const MAX_NOTE_BYTES: usize = 256 * 1024;

/// A span of inline text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Inline {
    Text { text: String },
    Emphasis { text: String },
    Strong { text: String },
    Code { text: String },
    Link { text: String, url: String },
}

impl Inline {
    fn text(&self) -> &str {
        match self {
            Self::Text { text }
            | Self::Emphasis { text }
            | Self::Strong { text }
            | Self::Code { text }
            | Self::Link { text, .. } => text,
        }
    }
}

/// A block of liner notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    Paragraph {
        content: Vec<Inline>,
    },
    Quote {
        blocks: Vec<Block>,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<Inline>>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Rule,
}

/// Reject notes that are too large or contain a NUL byte. Both are storage
/// problems, not formatting ones, so they are checked before the editor saves.
pub fn validate_notes(notes: &str) -> Result<(), crate::MediaError> {
    if notes.len() > MAX_NOTE_BYTES {
        return Err(crate::MediaError::InvalidInput(format!(
            "liner notes are {} bytes; the limit is {MAX_NOTE_BYTES}",
            notes.len()
        )));
    }
    if notes.contains('\0') {
        return Err(crate::MediaError::InvalidInput(
            "liner notes contain a NUL byte".to_string(),
        ));
    }
    Ok(())
}

/// Parse Markdown into display blocks.
pub fn parse(input: &str) -> Vec<Block> {
    let normalised = input.replace("\r\n", "\n").replace('\r', "\n");
    parse_lines(&normalised)
}

fn parse_lines(input: &str) -> Vec<Block> {
    let lines: Vec<&str> = input.split('\n').collect();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        if let Some((fence, language)) = fence_start(trimmed) {
            let mut code = Vec::new();
            index += 1;
            while index < lines.len() {
                let candidate = lines[index].trim();
                if is_closing_fence(candidate, fence) {
                    index += 1;
                    break;
                }
                code.push(lines[index]);
                index += 1;
            }
            blocks.push(Block::Code {
                language,
                text: code.join("\n"),
            });
            continue;
        }

        if let Some((level, text)) = heading(trimmed) {
            blocks.push(Block::Heading {
                level,
                content: parse_inline(text),
            });
            index += 1;
            continue;
        }

        if is_rule(trimmed) {
            blocks.push(Block::Rule);
            index += 1;
            continue;
        }

        if trimmed.starts_with('>') {
            let mut quoted = Vec::new();
            while index < lines.len() {
                let candidate = lines[index].trim_start();
                let Some(rest) = candidate.strip_prefix('>') else {
                    break;
                };
                quoted.push(rest.strip_prefix(' ').unwrap_or(rest));
                index += 1;
            }
            blocks.push(Block::Quote {
                blocks: parse_lines(&quoted.join("\n")),
            });
            continue;
        }

        if list_marker(trimmed).is_some() {
            let (block, next) = parse_list(&lines, index);
            blocks.push(block);
            index = next;
            continue;
        }

        // A paragraph: consecutive ordinary lines. Two trailing spaces or a
        // trailing backslash make a hard break; otherwise lines join with a
        // space so wrapped source stays one sentence.
        let mut text = String::new();
        while index < lines.len() {
            let candidate = lines[index];
            let candidate_trimmed = candidate.trim();
            if candidate_trimmed.is_empty()
                || fence_start(candidate_trimmed).is_some()
                || heading(candidate_trimmed).is_some()
                || is_rule(candidate_trimmed)
                || candidate_trimmed.starts_with('>')
                || list_marker(candidate_trimmed).is_some()
            {
                break;
            }
            let hard_break = candidate.ends_with("  ") || candidate.ends_with('\\');
            let piece = if hard_break {
                candidate.trim_end_matches('\\').trim_end()
            } else {
                candidate_trimmed
            };
            if !text.is_empty() {
                text.push(if hard_break { '\n' } else { ' ' });
            }
            text.push_str(piece);
            index += 1;
        }
        if !text.is_empty() {
            blocks.push(Block::Paragraph {
                content: parse_inline(&text),
            });
        } else {
            // Defensive: always make progress even if a line looked special to
            // one predicate and ordinary to another.
            index += 1;
        }
    }
    blocks
}

fn parse_list(lines: &[&str], mut index: usize) -> (Block, usize) {
    let first = lines[index].trim();
    let (ordered, mut start) = match list_marker(first).expect("caller checked the marker") {
        ListMarker::Ordered(number) => (true, number),
        ListMarker::Bullet => (false, 1),
    };
    let mut items: Vec<Vec<Inline>> = Vec::new();
    let mut current = String::new();

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // One blank line may separate items of the same kind. A switch
            // from bullets to numbers (or the reverse) starts a new list.
            let continues = index + 1 < lines.len()
                && matches!(
                    (ordered, list_marker(lines[index + 1].trim())),
                    (false, Some(ListMarker::Bullet)) | (true, Some(ListMarker::Ordered(_)))
                );
            if continues {
                index += 1;
                continue;
            }
            break;
        }
        let marker = list_marker(trimmed);
        let indented = line.starts_with("  ") || line.starts_with('\t');
        match marker {
            Some(ListMarker::Bullet) if !indented => {
                if !current.is_empty() {
                    items.push(parse_inline(current.trim()));
                }
                current = trimmed
                    .trim_start_matches(['-', '*', '+'])
                    .trim_start()
                    .to_string();
                index += 1;
            }
            Some(ListMarker::Ordered(number)) if !indented => {
                if !current.is_empty() {
                    items.push(parse_inline(current.trim()));
                }
                if items.is_empty() {
                    start = number;
                }
                current = trimmed
                    .split_once('.')
                    .map(|(_, rest)| rest.trim_start())
                    .unwrap_or("")
                    .to_string();
                index += 1;
            }
            _ if indented => {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(trimmed);
                index += 1;
            }
            _ => break,
        }
    }
    if !current.is_empty() {
        items.push(parse_inline(current.trim()));
    }
    (
        Block::List {
            ordered,
            start,
            items,
        },
        index,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListMarker {
    Bullet,
    Ordered(u64),
}

fn list_marker(line: &str) -> Option<ListMarker> {
    let first = line.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        let rest = line[1..].strip_prefix(' ')?;
        return (!rest.is_empty()).then_some(ListMarker::Bullet);
    }
    let digit_count = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_count == 0 || digit_count > 9 {
        return None;
    }
    let (number, rest) = line.split_at(digit_count);
    let rest = rest.strip_prefix('.')?.strip_prefix(' ')?;
    (!rest.is_empty()).then(|| ListMarker::Ordered(number.parse().unwrap_or(1)))
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let level = line.chars().take_while(|ch| *ch == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if rest.is_empty() {
        return Some((level as u8, ""));
    }
    let text = rest.strip_prefix(' ')?;
    // ATX closing hashes are decoration, not content.
    let text = text.trim_end();
    let text = text.trim_end_matches('#').trim_end();
    Some((level as u8, text))
}

fn fence_start(line: &str) -> Option<(&str, Option<String>)> {
    let trimmed = line.trim_start();
    let marker = trimmed
        .starts_with("```")
        .then_some("```")
        .or_else(|| trimmed.starts_with("~~~").then_some("~~~"))?;
    let info = trimmed[marker.len()..].trim();
    let language = info
        .split_whitespace()
        .next()
        .filter(|language| !language.is_empty())
        .map(|language| language.to_string());
    Some((marker, language))
}

fn is_closing_fence(line: &str, marker: &str) -> bool {
    line.starts_with(marker)
        && line
            .trim_matches(marker.chars().next().unwrap_or('`'))
            .trim()
            .is_empty()
}

fn is_rule(line: &str) -> bool {
    let compact: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    compact.len() >= 3
        && (compact.chars().all(|ch| ch == '-')
            || compact.chars().all(|ch| ch == '*')
            || compact.chars().all(|ch| ch == '_'))
}

/// Parse the inline subset.
pub fn parse_inline(input: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut text = String::new();

    let flush = |text: &mut String, out: &mut Vec<Inline>| {
        if !text.is_empty() {
            out.push(Inline::Text {
                text: std::mem::take(text),
            });
        }
    };

    while index < bytes.len() {
        let ch = input[index..].chars().next().unwrap();
        match ch {
            '\\' => {
                index += '\\'.len_utf8();
                if let Some(next) = input[index..].chars().next() {
                    if next.is_ascii_punctuation() {
                        text.push(next);
                        index += next.len_utf8();
                    } else {
                        text.push('\\');
                    }
                } else {
                    text.push('\\');
                }
            }
            '`' => {
                let ticks = count_run(input, index, '`');
                let marker = &input[index..index + ticks];
                if let Some(close) = find_run(input, index + ticks, '`', ticks) {
                    flush(&mut text, &mut out);
                    out.push(Inline::Code {
                        text: input[index + ticks..close].to_string(),
                    });
                    index = close + ticks;
                } else {
                    text.push_str(marker);
                    index += ticks;
                }
            }
            '*' | '_' => {
                let run = count_run(input, index, ch);
                let marker_len = if run >= 2 { 2 } else { 1 };
                let marker = &input[index..index + marker_len];
                if let Some(close) = find_run(input, index + marker_len, ch, marker_len) {
                    let inner = &input[index + marker_len..close];
                    if !inner.is_empty() {
                        flush(&mut text, &mut out);
                        let parsed = parse_inline(inner);
                        out.extend(if marker_len == 2 {
                            merge_strong(parsed)
                        } else {
                            merge_emphasis(parsed)
                        });
                        index = close + marker_len;
                    } else {
                        text.push_str(marker);
                        index += marker_len;
                    }
                } else {
                    text.push_str(marker);
                    index += marker_len;
                }
            }
            '!' if input[index..].starts_with("![") => {
                // Images are not part of liner notes; keep the alt text.
                if let Some((alt, _, end)) = parse_link(input, index + 1) {
                    text.push_str(&alt);
                    index = end;
                } else {
                    text.push('!');
                    index += 1;
                }
            }
            '[' => {
                if let Some((label, url, end)) = parse_link(input, index) {
                    flush(&mut text, &mut out);
                    if safe_link(&url) {
                        out.push(Inline::Link {
                            text: flatten_inline(&parse_inline(&label)),
                            url,
                        });
                    } else {
                        out.push(Inline::Text {
                            text: format!("{} ({})", flatten_inline(&parse_inline(&label)), url),
                        });
                    }
                    index = end;
                } else {
                    text.push('[');
                    index += 1;
                }
            }
            '<' => {
                if let Some((url, end)) = parse_autolink(input, index) {
                    flush(&mut text, &mut out);
                    if safe_link(&url) {
                        let label = url.strip_prefix("mailto:").unwrap_or(&url).to_string();
                        out.push(Inline::Link { text: label, url });
                    } else {
                        out.push(Inline::Text {
                            text: format!("<{url}>"),
                        });
                    }
                    index = end;
                } else {
                    text.push('<');
                    index += 1;
                }
            }
            _ => {
                text.push(ch);
                index += ch.len_utf8();
            }
        }
    }
    flush(&mut text, &mut out);
    merge_adjacent(out)
}

fn merge_strong(parsed: Vec<Inline>) -> Vec<Inline> {
    vec![Inline::Strong {
        text: flatten_inline(&parsed),
    }]
}

fn merge_emphasis(parsed: Vec<Inline>) -> Vec<Inline> {
    vec![Inline::Emphasis {
        text: flatten_inline(&parsed),
    }]
}

fn merge_adjacent(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines {
        if let (Some(Inline::Text { text: previous }), Inline::Text { text }) =
            (out.last_mut(), &inline)
        {
            previous.push_str(text);
        } else {
            out.push(inline);
        }
    }
    out
}

fn parse_link(input: &str, start: usize) -> Option<(String, String, usize)> {
    debug_assert!(input[start..].starts_with('['));
    let close = find_unescaped(input, start + 1, ']')?;
    let mut cursor = close + 1;
    if !input[cursor..].starts_with('(') {
        return None;
    }
    cursor += 1;
    let mut depth = 1;
    let url_start = cursor;
    while cursor < input.len() {
        let ch = input[cursor..].chars().next()?;
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let url = input[url_start..cursor].trim();
                    let url = url
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_matches(['<', '>']);
                    return Some((
                        input[start + 1..close].to_string(),
                        url.to_string(),
                        cursor + 1,
                    ));
                }
            }
            _ => {}
        }
        cursor += ch.len_utf8();
    }
    None
}

fn parse_autolink(input: &str, start: usize) -> Option<(String, usize)> {
    let close = input[start + 1..].find('>')? + start + 1;
    let url = &input[start + 1..close];
    (!url.is_empty() && !url.chars().any(char::is_whitespace)).then(|| (url.to_string(), close + 1))
}

fn find_unescaped(input: &str, start: usize, needle: char) -> Option<usize> {
    let mut escaped = false;
    for (offset, ch) in input[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
        } else if ch == needle {
            return Some(start + offset);
        }
    }
    None
}

fn count_run(input: &str, start: usize, needle: char) -> usize {
    input[start..]
        .chars()
        .take_while(|ch| *ch == needle)
        .map(char::len_utf8)
        .sum()
}

fn find_run(input: &str, start: usize, needle: char, length: usize) -> Option<usize> {
    let mut cursor = start;
    while cursor < input.len() {
        if input[cursor..].starts_with(needle) {
            let run = count_run(input, cursor, needle);
            if run >= length {
                return Some(cursor);
            }
            cursor += run;
        } else {
            cursor += input[cursor..].chars().next()?.len_utf8();
        }
    }
    None
}

/// Only schemes a notes file may turn into a clickable button.
pub fn safe_link(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

/// Flatten inline spans to their visible text, for search, summaries and
/// accessibility.
pub fn flatten_inline(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        out.push_str(inline.text());
    }
    out
}

/// Flatten the whole document to readable text. Block boundaries become
/// newlines; list items get a bullet or number.
pub fn plain_text(input: &str) -> String {
    let mut out = String::new();
    write_blocks(&parse(input), &mut out);
    out.trim().to_string()
}

fn write_blocks(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match block {
            Block::Heading { content, .. } | Block::Paragraph { content } => {
                out.push_str(&flatten_inline(content));
                out.push('\n');
            }
            Block::Quote { blocks } => {
                write_blocks(blocks, out);
            }
            Block::List {
                ordered,
                start,
                items,
            } => {
                for (index, item) in items.iter().enumerate() {
                    if *ordered {
                        out.push_str(&format!("{}. ", start + index as u64));
                    } else {
                        out.push_str("• ");
                    }
                    out.push_str(&flatten_inline(item));
                    out.push('\n');
                }
            }
            Block::Code { text, .. } => {
                out.push_str(text);
                out.push('\n');
            }
            Block::Rule => {}
        }
    }
}

/// A short preview for a card: the first heading or paragraph, without markup.
pub fn summary(input: &str, max_chars: usize) -> String {
    let flat = plain_text(input);
    let flat = flat.replace('\n', " ");
    let flat = flat.trim();
    if flat.chars().count() <= max_chars {
        return flat.to_string();
    }
    let mut out: String = flat.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph_text(block: &Block) -> String {
        match block {
            Block::Paragraph { content } => flatten_inline(content),
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn headings_and_paragraphs_keep_their_structure() {
        let blocks = parse("# Side One\n\nA warm, close recording.\n\n### Notes");
        assert_eq!(blocks.len(), 3);
        assert!(matches!(
            &blocks[0],
            Block::Heading { level: 1, content } if flatten_inline(content) == "Side One"
        ));
        assert_eq!(paragraph_text(&blocks[1]), "A warm, close recording.");
        assert!(matches!(&blocks[2], Block::Heading { level: 3, .. }));
    }

    #[test]
    fn emphasis_strong_code_and_links_are_structured() {
        let blocks = parse(
            "This is *quiet*, **loud**, `44.1 kHz`, and [the source](https://example.com/notes).",
        );
        let content = match &blocks[0] {
            Block::Paragraph { content } => content,
            other => panic!("expected paragraph, got {other:?}"),
        };
        assert!(content
            .iter()
            .any(|item| matches!(item, Inline::Emphasis { text } if text == "quiet")));
        assert!(content
            .iter()
            .any(|item| matches!(item, Inline::Strong { text } if text == "loud")));
        assert!(content
            .iter()
            .any(|item| matches!(item, Inline::Code { text } if text == "44.1 kHz")));
        assert!(content.iter().any(|item| matches!(
            item,
            Inline::Link { text, url } if text == "the source" && url == "https://example.com/notes"
        )));
    }

    #[test]
    fn unsafe_links_never_become_links() {
        let blocks = parse("[click](javascript:alert(1)) and <file:///etc/passwd>");
        let blocks_text = blocks
            .iter()
            .map(|block| match block {
                Block::Paragraph { content } => flatten_inline(content),
                _ => String::new(),
            })
            .collect::<String>();
        assert!(blocks_text.contains("javascript:alert(1)"));
        assert!(blocks_text.contains("file:///etc/passwd"));
        assert!(!blocks.iter().any(|block| match block {
            Block::Paragraph { content } => content
                .iter()
                .any(|inline| matches!(inline, Inline::Link { .. })),
            _ => false,
        }));
    }

    #[test]
    fn lists_quotes_and_rules_parse() {
        let blocks = parse("- one\n- two\n\n1. first\n2. second\n\n> A quote\n\n---");
        assert!(matches!(
            &blocks[0],
            Block::List { ordered: false, items, .. } if items.len() == 2
        ));
        assert!(matches!(
            &blocks[1],
            Block::List { ordered: true, start: 1, items, .. } if items.len() == 2
        ));
        assert!(matches!(&blocks[2], Block::Quote { blocks } if !blocks.is_empty()));
        assert!(matches!(&blocks[3], Block::Rule));
    }

    #[test]
    fn fenced_code_is_not_formatted_as_prose() {
        let blocks = parse("```rust\nlet x = **not bold**;\n```\n");
        match &blocks[0] {
            Block::Code { language, text } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert_eq!(text, "let x = **not bold**;");
            }
            other => panic!("expected code, got {other:?}"),
        }
    }

    #[test]
    fn escapes_and_unclosed_markers_stay_visible() {
        let blocks = parse(r"5 \* 3 and a lone * marker");
        let text = paragraph_text(&blocks[0]);
        assert!(text.contains("5 * 3"));
        assert!(text.contains("lone * marker"));
    }

    #[test]
    fn plain_text_is_readable_and_has_no_markup() {
        let text = plain_text("# Title\n\nA **bold** claim.\n\n- one\n- two");
        assert_eq!(text, "Title\nA bold claim.\n• one\n• two");
    }

    #[test]
    fn summary_is_bounded_on_character_boundaries() {
        assert_eq!(
            summary("# Hello\n\nA useful note.", 100),
            "Hello A useful note."
        );
        let shortened = summary("αααααααααα", 5);
        assert_eq!(shortened, "αααα…");
    }

    #[test]
    fn oversized_notes_are_refused() {
        let huge = "x".repeat(MAX_NOTE_BYTES + 1);
        assert!(validate_notes(&huge).is_err());
        assert!(validate_notes("a\0b").is_err());
        validate_notes("ordinary notes").unwrap();
    }
}
