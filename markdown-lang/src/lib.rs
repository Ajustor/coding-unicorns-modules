//! Markdown language module: highlighting for CommonMark and GitHub Flavored
//! Markdown, plus hover on link references and footnotes.
//!
//! The IDE colours tokens by kind (no bold or italics), so Markdown elements
//! are mapped onto the available kinds:
//!
//! | Element                                              | Kind       |
//! |------------------------------------------------------|------------|
//! | Headings (ATX `#` and setext underlined)             | `keyword`  |
//! | Block markers: lists, `>`, `---`, fences, table pipes | `class`    |
//! | `*emphasis*` / `_emphasis_`                          | `type`     |
//! | `**strong**` / `__strong__`                          | `function` |
//! | `~~strikethrough~~`, `<!-- comments -->`             | `comment`  |
//! | `` `code` ``, fenced and indented code blocks        | `string`   |
//! | Link and image text `[…]`, footnotes `[^1]`          | `property` |
//! | URLs: `(…)` of links, `<autolinks>`, bare `https://` | `number`   |
//! | Inline HTML tags                                     | `macro`    |
//! | Task boxes `[ ]` / `[x]`                             | `keyword`  |
//!
//! Front matter (`---` YAML or `+++` TOML on the first line) shows its keys
//! as `property` and values as `string`.

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};

/// Token kinds understood by the IDE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Heading,
    Marker,
    Emphasis,
    Strong,
    Faded,
    Code,
    LinkText,
    Url,
    Html,
    Normal,
    /// Front matter key.
    Key,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Heading => "keyword",
            Kind::Marker => "class",
            Kind::Emphasis => "type",
            Kind::Strong => "function",
            Kind::Faded => "comment",
            Kind::Code => "string",
            Kind::LinkText | Kind::Key => "property",
            Kind::Url => "number",
            Kind::Html => "macro",
            Kind::Normal => "normal",
        }
    }
}

type Line = Vec<(String, Kind)>;

/// Add `text` to `line`, merged with the previous token of the same kind.
fn push(line: &mut Line, text: &[char], kind: Kind) {
    if text.is_empty() {
        return;
    }
    match line.last_mut() {
        Some((prev, k)) if *k == kind => prev.extend(text),
        _ => line.push((text.iter().collect(), kind)),
    }
}

// ── Document scan ─────────────────────────────────────────────────────────────

/// Link reference definitions (`[label]: url "title"`) and footnotes
/// (`[^label]: text`) of a document, keyed by lowercase label.
#[derive(Debug, Default)]
pub struct Definitions {
    pub links: HashMap<String, (String, String)>,
    pub footnotes: HashMap<String, String>,
}

/// Collect the document's definitions (outside code blocks).
pub fn definitions(source: &str) -> Definitions {
    let mut defs = Definitions::default();
    let mut fence: Option<(char, usize)> = None;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some((ch, len)) = fence {
            if is_closing_fence(trimmed, ch, len) {
                fence = None;
            }
            continue;
        }
        if let Some((ch, len, _)) = opening_fence(trimmed) {
            fence = Some((ch, len));
            continue;
        }
        if indent_width(line) > 3 {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix('[') else {
            continue;
        };
        let Some(close) = rest.find("]:") else {
            continue;
        };
        let label = &rest[..close];
        let body = rest[close + 2..].trim();
        if let Some(note) = label.strip_prefix('^') {
            defs.footnotes.insert(note.to_lowercase(), body.to_string());
        } else if !label.is_empty() && !body.is_empty() {
            let (url, title) = split_url_title(body);
            defs.links
                .insert(label.to_lowercase(), (url.to_string(), title.to_string()));
        }
    }
    defs
}

/// `<url> "title"` / `url 'title'` / `url (title)` → (url, title).
fn split_url_title(body: &str) -> (&str, &str) {
    let (url, rest) = if let Some(inner) = body.strip_prefix('<') {
        match inner.find('>') {
            Some(end) => (&inner[..end], &inner[end + 1..]),
            None => (body, ""),
        }
    } else {
        match body.find(char::is_whitespace) {
            Some(end) => (&body[..end], &body[end..]),
            None => (body, ""),
        }
    };
    let title = rest.trim();
    let title = title
        .strip_prefix(['"', '\'', '('])
        .and_then(|t| t.strip_suffix(['"', '\'', ')']))
        .unwrap_or(title);
    (url, title)
}

/// Block-level state carried from line to line.
#[derive(Default)]
struct State {
    /// Inside front matter: the closing delimiter.
    front_matter: Option<&'static str>,
    /// Inside a fenced code block: fence character and length.
    fence: Option<(char, usize)>,
    /// Inside a `<!-- … -->` comment spanning lines.
    html_comment: bool,
    /// The next line underlines a setext heading.
    setext_underline: bool,
    /// Inside a table (pipes are markers).
    table: bool,
    /// The previous line is paragraph text (an indented line continues it).
    paragraph: bool,
    /// Inside a list (indented lines belong to its items, not code blocks).
    list: bool,
    /// The previous line is blank.
    blank: bool,
}

/// Tokens of each line of `source` (one vector per line, line breaks excluded).
pub fn tokenize(source: &str) -> Vec<Line> {
    let defs = definitions(source);
    let lines: Vec<&str> = source
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut st = State {
        blank: true,
        ..State::default()
    };
    // Front matter: `---` (YAML) or `+++` (TOML) on the first line, closed later.
    let mut skip = 0;
    if let Some(&open @ ("---" | "+++")) = lines.first() {
        let closed = lines[1..]
            .iter()
            .any(|l| l.trim_end() == open || (open == "---" && l.trim_end() == "..."));
        if closed {
            st.front_matter = Some(if open == "---" { "---" } else { "+++" });
            skip = 1;
        }
    }
    let mut out: Vec<Line> = lines[..skip]
        .iter()
        .map(|l| vec![(l.to_string(), Kind::Marker)])
        .collect();
    for n in skip..lines.len() {
        let chars: Vec<char> = lines[n].chars().collect();
        let next = lines.get(n + 1).copied();
        out.push(tokenize_line_in(&mut st, &defs, &chars, next));
    }
    out
}

fn tokenize_line_in(
    st: &mut State,
    defs: &Definitions,
    chars: &[char],
    next: Option<&str>,
) -> Line {
    let mut out = Line::new();
    let text: String = chars.iter().collect();
    let trimmed = text.trim();

    if st.html_comment {
        match find(chars, 0, &['-', '-', '>']) {
            Some(end) => {
                push(&mut out, &chars[..end + 3], Kind::Faded);
                st.html_comment = false;
                inline(&chars[end + 3..], st, defs, &mut out);
            }
            None => push(&mut out, chars, Kind::Faded),
        }
        return out;
    }

    if let Some(close) = st.front_matter {
        if trimmed == close || (close == "---" && trimmed == "...") {
            push(&mut out, chars, Kind::Marker);
            st.front_matter = None;
        } else {
            front_matter_line(chars, &mut out);
        }
        return out;
    }

    if let Some((ch, len)) = st.fence {
        if is_closing_fence(text.trim_start(), ch, len) && indent_width(&text) < 4 {
            push(&mut out, chars, Kind::Marker);
            st.fence = None;
        } else {
            push(&mut out, chars, Kind::Code);
        }
        return out;
    }

    if trimmed.is_empty() {
        push(&mut out, chars, Kind::Normal);
        st.blank = true;
        st.paragraph = false;
        st.table = false;
        return out;
    }

    if st.setext_underline {
        st.setext_underline = false;
        st.paragraph = false;
        st.blank = false;
        push(&mut out, chars, Kind::Marker);
        return out;
    }

    let indent = indent_width(&text);
    if indent >= 4 && !st.paragraph && !st.list {
        push(&mut out, chars, Kind::Code);
        st.blank = false;
        return out;
    }
    if indent == 0 && st.blank && is_list_item(chars).is_none() {
        st.list = false;
    }
    st.blank = false;

    // Block quote markers, possibly nested (`> > text`).
    let mut i = 0;
    loop {
        let ws = skip_spaces(chars, i);
        if ws - i <= 3 && chars.get(ws) == Some(&'>') {
            push(&mut out, &chars[i..ws], Kind::Normal);
            push(&mut out, &chars[ws..ws + 1], Kind::Marker);
            i = ws + 1;
            if chars.get(i) == Some(&' ') {
                push(&mut out, &chars[i..i + 1], Kind::Normal);
                i += 1;
            }
        } else {
            break;
        }
    }
    let rest = &chars[i..];
    let rest_text: String = rest.iter().collect();
    let rest_trimmed = rest_text.trim_start();
    let lead = rest.len() - rest_trimmed.chars().count();

    // Fenced code block opening.
    if let Some((ch, len, info)) = opening_fence(rest_trimmed) {
        if lead < 4 {
            push(&mut out, &rest[..lead], Kind::Normal);
            push(&mut out, &rest[lead..lead + len], Kind::Marker);
            let info: Vec<char> = info.chars().collect();
            push(&mut out, &info, Kind::LinkText);
            st.fence = Some((ch, len));
            st.paragraph = false;
            return out;
        }
    }

    // ATX heading.
    let hashes = rest_trimmed.chars().take_while(|&c| c == '#').count();
    if lead < 4
        && (1..=6).contains(&hashes)
        && rest_trimmed[hashes..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    {
        push(&mut out, &rest[..lead], Kind::Normal);
        push(&mut out, &rest[lead..], Kind::Heading);
        st.paragraph = false;
        return out;
    }

    // Thematic break (`---`, `***`, `___`); a setext underline was handled above.
    if lead < 4 && is_thematic_break(rest_trimmed) {
        push(&mut out, rest, Kind::Marker);
        st.paragraph = false;
        return out;
    }

    // Table delimiter row (`| --- | :-: |`).
    if st.table && is_table_delimiter(rest_trimmed) {
        push(&mut out, rest, Kind::Marker);
        return out;
    }

    // Link reference / footnote definition.
    if lead < 4 && !st.paragraph {
        if let Some(def) = definition_line(rest, lead, st, defs) {
            out.extend(def);
            return out;
        }
    }

    // List item: marker, optional task box, then inline content.
    let mut body_start = 0;
    if let Some((marker_start, marker_end)) = is_list_item(rest) {
        push(&mut out, &rest[..marker_start], Kind::Normal);
        push(&mut out, &rest[marker_start..marker_end], Kind::Marker);
        body_start = marker_end;
        let after = skip_spaces(rest, body_start);
        push(&mut out, &rest[body_start..after], Kind::Normal);
        body_start = after;
        if let Some(task) = task_box(&rest[body_start..]) {
            push(
                &mut out,
                &rest[body_start..body_start + task],
                Kind::Heading,
            );
            body_start += task;
        }
        st.list = true;
    }
    let body = &rest[body_start..];

    // Paragraph text: a setext heading when the next line underlines it,
    // a table header when the next line is a delimiter row.
    if i == 0 && body_start == 0 && !st.table && next.is_some_and(is_setext_underline) {
        push(&mut out, body, Kind::Heading);
        st.setext_underline = true;
        return out;
    }
    if !st.table && rest_text.contains('|') && next.is_some_and(|nx| is_table_delimiter(nx.trim()))
    {
        st.table = true;
    }
    inline(body, st, defs, &mut out);
    st.paragraph = true;
    out
}

fn front_matter_line(chars: &[char], out: &mut Line) {
    let start = skip_spaces(chars, 0);
    push(out, &chars[..start], Kind::Normal);
    let rest = &chars[start..];
    if rest.first() == Some(&'#') {
        push(out, rest, Kind::Faded);
        return;
    }
    if rest.first() == Some(&'-') && rest.get(1).is_none_or(|c| *c == ' ') {
        push(out, &rest[..1], Kind::Marker);
        push(out, &rest[1..], Kind::Code);
        return;
    }
    let key_end = rest
        .iter()
        .position(|c| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '"' | '\'')));
    match key_end {
        Some(k) if k > 0 && matches!(rest[k..].iter().find(|c| **c != ' '), Some(':' | '=')) => {
            push(out, &rest[..k], Kind::Key);
            let sep = k
                + rest[k..]
                    .iter()
                    .position(|c| *c == ':' || *c == '=')
                    .unwrap()
                + 1;
            push(out, &rest[k..sep], Kind::Normal);
            push(out, &rest[sep..], Kind::Code);
        }
        _ => push(out, rest, Kind::Code),
    }
}

/// `[label]: url "title"` or `[^note]: text` at the start of `rest` (after
/// `lead` spaces), as tokens.
fn definition_line(rest: &[char], lead: usize, st: &mut State, defs: &Definitions) -> Option<Line> {
    if rest.get(lead) != Some(&'[') {
        return None;
    }
    let close = (lead + 1..rest.len()).find(|&i| rest[i] == ']' && rest[i - 1] != '\\')?;
    if rest.get(close + 1) != Some(&':') || close == lead + 1 {
        return None;
    }
    let mut out = Line::new();
    push(&mut out, &rest[..lead], Kind::Normal);
    push(&mut out, &rest[lead..=close], Kind::LinkText);
    push(&mut out, &rest[close + 1..close + 2], Kind::Normal);
    let body = &rest[close + 2..];
    if rest[lead + 1] == '^' {
        inline(body, st, defs, &mut out);
        st.paragraph = true;
        return Some(out);
    }
    let ws = skip_spaces(body, 0);
    push(&mut out, &body[..ws], Kind::Normal);
    let url_end = if body.get(ws) == Some(&'<') {
        (ws..body.len())
            .find(|&i| body[i] == '>')
            .map_or(body.len(), |i| i + 1)
    } else {
        (ws..body.len())
            .find(|&i| body[i].is_whitespace())
            .unwrap_or(body.len())
    };
    push(&mut out, &body[ws..url_end], Kind::Url);
    push(&mut out, &body[url_end..], Kind::Code);
    st.paragraph = false;
    Some(out)
}

// ── Inline ────────────────────────────────────────────────────────────────────

fn inline(chars: &[char], st: &mut State, defs: &Definitions, out: &mut Line) {
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        match c {
            '\\' if chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) => {
                push(out, &chars[i..i + 2], Kind::Normal);
                i += 2;
            }
            '`' => {
                let n = run(chars, i, '`');
                match find_run(chars, i + n, '`', n) {
                    Some(end) => {
                        push(out, &chars[i..end + n], Kind::Code);
                        i = end + n;
                    }
                    None => {
                        push(out, &chars[i..i + n], Kind::Normal);
                        i += n;
                    }
                }
            }
            '<' => {
                if chars[i..].starts_with(&['<', '!', '-', '-']) {
                    match find(chars, i + 4, &['-', '-', '>']) {
                        Some(end) => {
                            push(out, &chars[i..end + 3], Kind::Faded);
                            i = end + 3;
                        }
                        None => {
                            push(out, &chars[i..], Kind::Faded);
                            st.html_comment = true;
                            i = chars.len();
                        }
                    }
                } else if let Some(end) = autolink(chars, i) {
                    push(out, &chars[i..end], Kind::Url);
                    i = end;
                } else if let Some(end) = html_tag(chars, i) {
                    push(out, &chars[i..end], Kind::Html);
                    i = end;
                } else {
                    push(out, &chars[i..i + 1], Kind::Normal);
                    i += 1;
                }
            }
            '!' | '[' => match link(chars, i, defs) {
                Some((text_end, end)) => {
                    push(out, &chars[i..text_end], Kind::LinkText);
                    push(out, &chars[text_end..end], Kind::Url);
                    i = end;
                }
                None => {
                    push(out, &chars[i..i + 1], Kind::Normal);
                    i += 1;
                }
            },
            'h' | 'w' if !prev.is_some_and(|p| p.is_alphanumeric()) => match bare_url(chars, i) {
                Some(end) => {
                    push(out, &chars[i..end], Kind::Url);
                    i = end;
                }
                None => {
                    push(out, &chars[i..i + 1], Kind::Normal);
                    i += 1;
                }
            },
            '*' | '_' | '~' => {
                let n = run(chars, i, c);
                match emphasis(chars, i, n) {
                    Some((end, kind)) => {
                        push(out, &chars[i..end], kind);
                        i = end;
                    }
                    None => {
                        push(out, &chars[i..i + n], Kind::Normal);
                        i += n;
                    }
                }
            }
            '|' if st.table => {
                push(out, &chars[i..i + 1], Kind::Marker);
                i += 1;
            }
            _ => {
                push(out, &chars[i..i + 1], Kind::Normal);
                i += 1;
            }
        }
    }
}

/// Length of the run of `c` starting at `i`.
fn run(chars: &[char], i: usize, c: char) -> usize {
    chars[i..].iter().take_while(|&&x| x == c).count()
}

/// Start of the next run of exactly `n` × `c` at or after `from`.
fn find_run(chars: &[char], from: usize, c: char, n: usize) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        if chars[j] == c {
            let len = run(chars, j, c);
            if len == n {
                return Some(j);
            }
            j += len;
        } else {
            j += 1;
        }
    }
    None
}

/// Index of `pat` in `chars` at or after `from`.
fn find(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i..].starts_with(pat))
}

/// `*em*`, `**strong**`, `~~strike~~` starting at `i` (a run of `n` delimiters):
/// end index and kind.
fn emphasis(chars: &[char], i: usize, n: usize) -> Option<(usize, Kind)> {
    let c = chars[i];
    let after = *chars.get(i + n)?;
    if after.is_whitespace() {
        return None;
    }
    // `snake_case_words` are not emphasis.
    if c == '_' && i > 0 && chars[i - 1].is_alphanumeric() {
        return None;
    }
    let kind = match (c, n) {
        ('~', 1 | 2) => Kind::Faded,
        ('~', _) => return None,
        (_, 1) => Kind::Emphasis,
        _ => Kind::Strong,
    };
    let mut j = i + n;
    while let Some(k) = find_run(chars, j, c, n) {
        let closes = !chars[k - 1].is_whitespace()
            && k > i + n
            && !(c == '_' && chars.get(k + n).is_some_and(|x| x.is_alphanumeric()));
        if closes {
            return Some((k + n, kind));
        }
        j = k + n;
    }
    None
}

/// `<https://…>` or `<user@host>` starting at `i`: end index.
fn autolink(chars: &[char], i: usize) -> Option<usize> {
    let end =
        (i + 1..chars.len()).find(|&j| chars[j] == '>' || chars[j] == '<' || chars[j] == ' ')?;
    if chars[end] != '>' {
        return None;
    }
    let inner: String = chars[i + 1..end].iter().collect();
    let scheme = inner.split_once(':').is_some_and(|(s, _)| {
        s.len() >= 2
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    });
    let email = inner.contains('@') && !inner.starts_with('@') && inner.contains('.');
    (scheme || email).then_some(end + 1)
}

/// `<tag …>`, `</tag>` or `<tag/>` starting at `i`: end index.
fn html_tag(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    if chars.get(j) == Some(&'/') {
        j += 1;
    }
    if !chars.get(j)?.is_ascii_alphabetic() {
        return None;
    }
    let end = (j..chars.len()).find(|&k| chars[k] == '>' || chars[k] == '<')?;
    (chars[end] == '>').then_some(end + 1)
}

/// `[text](url)`, `![alt](src)`, `[text][ref]`, `[ref]` (when defined) or
/// `[^note]` starting at `i`: (end of the text part, end).
fn link(chars: &[char], i: usize, defs: &Definitions) -> Option<(usize, usize)> {
    let open = if chars[i] == '!' { i + 1 } else { i };
    if chars.get(open) != Some(&'[') {
        return None;
    }
    // Matching `]`, brackets nested, escapes and code spans skipped.
    let mut depth = 0;
    let mut j = open;
    let close = loop {
        match chars.get(j)? {
            '\\' => j += 1,
            '`' => {
                let n = run(chars, j, '`');
                j = find_run(chars, j + n, '`', n).map_or(j + n - 1, |e| e + n - 1);
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break j;
                }
            }
            _ => {}
        }
        j += 1;
    };
    let label: String = chars[open + 1..close].iter().collect();
    let text_end = close + 1;
    match chars.get(text_end) {
        Some('(') => {
            let mut depth = 0;
            let mut k = text_end;
            loop {
                match chars.get(k)? {
                    '\\' => k += 1,
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((text_end, k + 1));
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
        }
        Some('[') => {
            let end = (text_end + 1..chars.len()).find(|&k| chars[k] == ']')?;
            Some((end + 1, end + 1))
        }
        _ => {
            let key = label.to_lowercase();
            let known = match key.strip_prefix('^') {
                Some(note) => !note.is_empty(),
                None => defs.links.contains_key(&key),
            };
            known.then_some((text_end, text_end))
        }
    }
}

/// A bare `https://…`, `http://…` or `www.…` URL starting at `i`: end index.
fn bare_url(chars: &[char], i: usize) -> Option<usize> {
    let rest = &chars[i..];
    let prefix = ["https://", "http://", "www."]
        .iter()
        .find(|p| rest.starts_with(&p.chars().collect::<Vec<_>>()))?;
    let mut end = (i..chars.len())
        .find(|&k| chars[k].is_whitespace() || chars[k] == '<')
        .unwrap_or(chars.len());
    // Trailing punctuation belongs to the sentence, and `)` unless balanced.
    loop {
        let last = chars[end - 1];
        let opens = chars[i..end].iter().filter(|&&c| c == '(').count();
        let closes = chars[i..end].iter().filter(|&&c| c == ')').count();
        if "?!.,:*_~'\";".contains(last) || (last == ')' && closes > opens) {
            end -= 1;
        } else {
            break;
        }
    }
    (end > i + prefix.len()).then_some(end)
}

// ── Block helpers ─────────────────────────────────────────────────────────────

/// Width of the leading whitespace (a tab counts 4).
fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

fn skip_spaces(chars: &[char], from: usize) -> usize {
    (from..chars.len())
        .find(|&i| chars[i] != ' ' && chars[i] != '\t')
        .unwrap_or(chars.len())
}

/// "```lang" / "~~~": fence character, length and info string.
fn opening_fence(trimmed: &str) -> Option<(char, usize, &str)> {
    let ch = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = trimmed.chars().take_while(|&c| c == ch).count();
    let info = &trimmed[len..];
    // A backtick fence's info string can't contain backticks (inline code).
    (len >= 3 && !(ch == '`' && info.contains('`'))).then_some((ch, len, info))
}

fn is_closing_fence(trimmed: &str, ch: char, len: usize) -> bool {
    let n = trimmed.chars().take_while(|&c| c == ch).count();
    n >= len && trimmed[n..].trim().is_empty()
}

fn is_thematic_break(trimmed: &str) -> bool {
    let mut chars = trimmed.chars().filter(|c| !c.is_whitespace());
    let Some(first) = chars.next() else {
        return false;
    };
    matches!(first, '-' | '*' | '_') && chars.clone().count() >= 2 && chars.all(|c| c == first)
}

fn is_setext_underline(line: &str) -> bool {
    if indent_width(line) > 3 {
        return false;
    }
    let t = line.trim();
    let Some(first) = t.chars().next() else {
        return false;
    };
    matches!(first, '=' | '-') && t.chars().all(|c| c == first)
}

/// `| --- | :-: | --: |` (at least one pipe).
fn is_table_delimiter(trimmed: &str) -> bool {
    let trimmed = trimmed.trim();
    if !trimmed.contains('|') {
        return false;
    }
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner.split('|').all(|cell| {
        let cell = cell.trim();
        let cell = cell.strip_prefix(':').unwrap_or(cell);
        let cell = cell.strip_suffix(':').unwrap_or(cell);
        !cell.is_empty() && cell.chars().all(|c| c == '-')
    })
}

/// `- `, `* `, `+ `, `1. `, `1) ` at the start of `chars`: (marker start,
/// marker end).
fn is_list_item(chars: &[char]) -> Option<(usize, usize)> {
    let start = skip_spaces(chars, 0);
    let c = *chars.get(start)?;
    let end = if matches!(c, '-' | '*' | '+') {
        start + 1
    } else {
        let digits = chars[start..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if !(1..=9).contains(&digits) || !matches!(chars.get(start + digits), Some('.' | ')')) {
            return None;
        }
        start + digits + 1
    };
    chars
        .get(end)
        .is_none_or(|c| *c == ' ' || *c == '\t')
        .then_some((start, end))
}

/// `[ ]`, `[x]` or `[X]` followed by a space or the end: its length.
fn task_box(chars: &[char]) -> Option<usize> {
    let is_box = chars.len() >= 3
        && chars[0] == '['
        && matches!(chars[1], ' ' | 'x' | 'X')
        && chars[2] == ']'
        && chars.get(3).is_none_or(|c| c.is_whitespace());
    is_box.then_some(3)
}

// ── Hover ─────────────────────────────────────────────────────────────────────

/// Hover text for `word` in a document: the target of a link reference or
/// the text of a footnote it names.
pub fn hover(word: &str, content: &str) -> Option<String> {
    let label = word
        .trim_matches(|c| c == '[' || c == ']')
        .trim_start_matches('^')
        .to_lowercase();
    if label.is_empty() {
        return None;
    }
    let defs = definitions(content);
    if let Some(note) = defs.footnotes.get(&label) {
        return Some(format!("**Footnote `[^{label}]`**\n\n{note}"));
    }
    let (url, title) = defs.links.get(&label)?;
    let mut doc = format!("**Link `[{label}]`** → <{url}>");
    if !title.is_empty() {
        doc.push_str(&format!("\n\n{title}"));
    }
    Some(doc)
}

// ── Serialization ─────────────────────────────────────────────────────────────

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn line_to_json(tokens: &[(String, Kind)]) -> String {
    let toks: Vec<String> = tokens
        .iter()
        .map(|(text, kind)| {
            format!(
                r#"{{"text":{},"kind":"{}"}}"#,
                json_escape(text),
                kind.as_str()
            )
        })
        .collect();
    format!("[{}]", toks.join(","))
}

fn document_to_json(source: &str) -> String {
    let lines: Vec<String> = tokenize(source).iter().map(|l| line_to_json(l)).collect();
    format!("[{}]", lines.join(","))
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

/// # Safety
/// `ptr` must be null or a valid NUL-terminated string.
unsafe fn from_c<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

// ── FFI ───────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const c_char {
    c"markdown".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"md,markdown,mdown,mkd,mkdn,mdwn".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// # Safety
/// `text_ptr` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    into_c(document_to_json(unsafe { from_c(text_ptr) }))
}

/// # Safety
/// `line_ptr` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_line_ffi(line_ptr: *const c_char) -> *mut c_char {
    let line = unsafe { from_c(line_ptr) };
    let tokens = tokenize(line).into_iter().next().unwrap_or_default();
    into_c(line_to_json(&tokens))
}

/// # Safety
/// Both pointers must be null or valid NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn hover_info_ffi(
    word_ptr: *const c_char,
    file_content_ptr: *const c_char,
) -> *mut c_char {
    let (word, content) = unsafe { (from_c(word_ptr), from_c(file_content_ptr)) };
    match hover(word, content) {
        Some(doc) => into_c(doc),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `ptr` must be null or a string returned by this module.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Non-blank tokens of a line, trimmed.
    fn kinds(line: &[(String, Kind)]) -> Vec<(&str, Kind)> {
        line.iter()
            .filter(|(t, _)| !t.trim().is_empty())
            .map(|(t, k)| (t.trim(), *k))
            .collect()
    }

    /// Non-blank tokens of a one-line document.
    fn one(src: &str) -> Vec<(String, Kind)> {
        tokenize(src)
            .remove(0)
            .into_iter()
            .filter(|(t, _)| !t.trim().is_empty())
            .map(|(t, k)| (t.trim().to_string(), k))
            .collect()
    }

    fn tok(t: &str, k: Kind) -> (String, Kind) {
        (t.to_string(), k)
    }

    #[test]
    fn lines_round_trip_their_text() {
        let src = "# T\r\n\r\n- a *b* `c`\n```rs\nfn x() {}\n```\n> q";
        let lines = tokenize(src);
        let expected: Vec<&str> = src.split('\n').map(|l| l.trim_end_matches('\r')).collect();
        assert_eq!(lines.len(), expected.len());
        for (line, text) in lines.iter().zip(expected) {
            let joined: String = line.iter().map(|(t, _)| t.as_str()).collect();
            assert_eq!(joined, text);
        }
    }

    #[test]
    fn atx_and_setext_headings() {
        let lines = tokenize("# Title\n###### Six\n####### seven\n#hashtag\nSub\n---\nMain\n===");
        assert_eq!(kinds(&lines[0]), [("# Title", Kind::Heading)]);
        assert_eq!(kinds(&lines[1]), [("###### Six", Kind::Heading)]);
        assert_eq!(kinds(&lines[2]), [("####### seven", Kind::Normal)]);
        assert_eq!(kinds(&lines[3]), [("#hashtag", Kind::Normal)]);
        assert_eq!(kinds(&lines[4]), [("Sub", Kind::Heading)]);
        assert_eq!(kinds(&lines[5]), [("---", Kind::Marker)]);
        assert_eq!(kinds(&lines[6]), [("Main", Kind::Heading)]);
        assert_eq!(kinds(&lines[7]), [("===", Kind::Marker)]);
    }

    #[test]
    fn thematic_break_after_a_blank_line_is_not_a_heading() {
        let lines = tokenize("Text\n\n---\n* * *");
        assert_eq!(kinds(&lines[0]), [("Text", Kind::Normal)]);
        assert_eq!(kinds(&lines[2]), [("---", Kind::Marker)]);
        assert_eq!(kinds(&lines[3]), [("* * *", Kind::Marker)]);
    }

    #[test]
    fn emphasis_strong_strike_and_code() {
        assert_eq!(
            one("a *em* **strong** ~~gone~~ `co*de*` _u_ __U__"),
            [
                tok("a", Kind::Normal),
                tok("*em*", Kind::Emphasis),
                tok("**strong**", Kind::Strong),
                tok("~~gone~~", Kind::Faded),
                tok("`co*de*`", Kind::Code),
                tok("_u_", Kind::Emphasis),
                tok("__U__", Kind::Strong),
            ]
        );
    }

    #[test]
    fn delimiters_that_are_not_emphasis() {
        assert_eq!(
            one("snake_case_name"),
            [tok("snake_case_name", Kind::Normal)]
        );
        assert_eq!(one("2 * 3 * 4"), [tok("2 * 3 * 4", Kind::Normal)]);
        assert_eq!(one(r"\*literal\*"), [tok(r"\*literal\*", Kind::Normal)]);
        assert_eq!(one("`open code"), [tok("`open code", Kind::Normal)]);
        assert_eq!(one("``a ` b``"), [tok("``a ` b``", Kind::Code)]);
    }

    #[test]
    fn links_images_and_urls() {
        assert_eq!(
            one("see [the docs](https://x.io/a_(b)) and ![logo](img.png \"t\")"),
            [
                tok("see", Kind::Normal),
                tok("[the docs]", Kind::LinkText),
                tok("(https://x.io/a_(b))", Kind::Url),
                tok("and", Kind::Normal),
                tok("![logo]", Kind::LinkText),
                tok("(img.png \"t\")", Kind::Url),
            ]
        );
        assert_eq!(
            one("go to https://example.com/path. Or <https://a.b> <me@x.org>"),
            [
                tok("go to", Kind::Normal),
                tok("https://example.com/path", Kind::Url),
                tok(". Or", Kind::Normal),
                tok("<https://a.b>", Kind::Url),
                tok("<me@x.org>", Kind::Url),
            ]
        );
        assert_eq!(one("(www.site.org)")[1], tok("www.site.org", Kind::Url));
    }

    #[test]
    fn reference_links_footnotes_and_definitions() {
        let src = "Use [Rust][rs], [rs] and [nope]. Note[^1].\n\n[rs]: https://rust-lang.org \"Rust\"\n[^1]: A *note*.";
        let lines = tokenize(src);
        assert_eq!(
            kinds(&lines[0]),
            [
                ("Use", Kind::Normal),
                ("[Rust][rs]", Kind::LinkText),
                (",", Kind::Normal),
                ("[rs]", Kind::LinkText),
                ("and [nope]. Note", Kind::Normal),
                ("[^1]", Kind::LinkText),
                (".", Kind::Normal),
            ]
        );
        assert_eq!(
            kinds(&lines[2]),
            [
                ("[rs]", Kind::LinkText),
                (":", Kind::Normal),
                ("https://rust-lang.org", Kind::Url),
                ("\"Rust\"", Kind::Code),
            ]
        );
        assert_eq!(
            kinds(&lines[3]),
            [
                ("[^1]", Kind::LinkText),
                (": A", Kind::Normal),
                ("*note*", Kind::Emphasis),
                (".", Kind::Normal),
            ]
        );
    }

    #[test]
    fn lists_tasks_and_quotes() {
        let lines =
            tokenize("- item\n  1. **n**\n* [x] done\n- [ ] todo\n> quote *em*\n> > - deep\n+ ");
        assert_eq!(
            kinds(&lines[0]),
            [("-", Kind::Marker), ("item", Kind::Normal)]
        );
        assert_eq!(
            kinds(&lines[1]),
            [("1.", Kind::Marker), ("**n**", Kind::Strong)]
        );
        assert_eq!(
            kinds(&lines[2]),
            [
                ("*", Kind::Marker),
                ("[x]", Kind::Heading),
                ("done", Kind::Normal)
            ]
        );
        assert_eq!(kinds(&lines[3])[1], ("[ ]", Kind::Heading));
        assert_eq!(
            kinds(&lines[4]),
            [
                (">", Kind::Marker),
                ("quote", Kind::Normal),
                ("*em*", Kind::Emphasis)
            ]
        );
        assert_eq!(
            kinds(&lines[5]),
            [
                (">", Kind::Marker),
                (">", Kind::Marker),
                ("-", Kind::Marker),
                ("deep", Kind::Normal),
            ]
        );
        assert_eq!(kinds(&lines[6]), [("+", Kind::Marker)]);
        assert_eq!(one("**bold** start")[0], tok("**bold**", Kind::Strong));
    }

    #[test]
    fn fenced_and_indented_code() {
        let src = "```rust\nlet *x* = 1;\n```\n~~~\n```\n~~~\n\n    indented\npara\n    continued";
        let lines = tokenize(src);
        assert_eq!(
            kinds(&lines[0]),
            [("```", Kind::Marker), ("rust", Kind::LinkText)]
        );
        assert_eq!(kinds(&lines[1]), [("let *x* = 1;", Kind::Code)]);
        assert_eq!(kinds(&lines[2]), [("```", Kind::Marker)]);
        assert_eq!(kinds(&lines[4]), [("```", Kind::Code)], "other fence kind");
        assert_eq!(kinds(&lines[5]), [("~~~", Kind::Marker)]);
        assert_eq!(kinds(&lines[7]), [("indented", Kind::Code)]);
        assert_eq!(
            kinds(&lines[9]),
            [("continued", Kind::Normal)],
            "paragraph continuation"
        );
    }

    #[test]
    fn indented_lines_in_lists_are_not_code() {
        let lines = tokenize("- item\n\n    more of the item");
        assert_eq!(kinds(&lines[2]), [("more of the item", Kind::Normal)]);
    }

    #[test]
    fn tables() {
        let lines = tokenize("| A | *B* |\n|---|:-:|\n| 1 | `2` |\n\nnot | a table");
        assert_eq!(
            kinds(&lines[0]),
            [
                ("|", Kind::Marker),
                ("A", Kind::Normal),
                ("|", Kind::Marker),
                ("*B*", Kind::Emphasis),
                ("|", Kind::Marker),
            ]
        );
        assert_eq!(kinds(&lines[1]), [("|---|:-:|", Kind::Marker)]);
        assert_eq!(kinds(&lines[2])[3], ("`2`", Kind::Code));
        assert_eq!(kinds(&lines[4]), [("not | a table", Kind::Normal)]);
    }

    #[test]
    fn html_and_comments() {
        let lines =
            tokenize("a <kbd>Ctrl</kbd> <!-- hidden --> b\n<!-- multi\nline -->after\n<br/> 1 < 2");
        assert_eq!(
            kinds(&lines[0]),
            [
                ("a", Kind::Normal),
                ("<kbd>", Kind::Html),
                ("Ctrl", Kind::Normal),
                ("</kbd>", Kind::Html),
                ("<!-- hidden -->", Kind::Faded),
                ("b", Kind::Normal),
            ]
        );
        assert_eq!(kinds(&lines[1]), [("<!-- multi", Kind::Faded)]);
        assert_eq!(
            kinds(&lines[2]),
            [("line -->", Kind::Faded), ("after", Kind::Normal)]
        );
        assert_eq!(
            kinds(&lines[3]),
            [("<br/>", Kind::Html), ("1 < 2", Kind::Normal)]
        );
    }

    #[test]
    fn front_matter() {
        let lines = tokenize("---\ntitle: \"Hello\"\n# comment\ntags:\n  - a\n---\n# Body");
        assert_eq!(kinds(&lines[0]), [("---", Kind::Marker)]);
        assert_eq!(
            kinds(&lines[1]),
            [
                ("title", Kind::Key),
                (":", Kind::Normal),
                ("\"Hello\"", Kind::Code)
            ]
        );
        assert_eq!(kinds(&lines[2]), [("# comment", Kind::Faded)]);
        assert_eq!(kinds(&lines[4]), [("-", Kind::Marker), ("a", Kind::Code)]);
        assert_eq!(kinds(&lines[5]), [("---", Kind::Marker)]);
        assert_eq!(kinds(&lines[6]), [("# Body", Kind::Heading)]);

        let toml = tokenize("+++\ndraft = true\n+++");
        assert_eq!(
            kinds(&toml[1]),
            [
                ("draft", Kind::Key),
                ("=", Kind::Normal),
                ("true", Kind::Code)
            ]
        );

        // Never closed: not front matter.
        let lines = tokenize("---\ntitle: x");
        assert_eq!(kinds(&lines[0]), [("---", Kind::Marker)]);
        assert_eq!(kinds(&lines[1]), [("title: x", Kind::Normal)]);
    }

    #[test]
    fn hover_on_references_and_footnotes() {
        let doc = "[RS]: <https://rust-lang.org> 'The Rust site'\n[bare]: https://x.io\n[^note]: Some text.\n```\n[code]: https://no\n```";
        let h = hover("rs", doc).unwrap();
        assert!(
            h.contains("https://rust-lang.org") && h.contains("The Rust site"),
            "{h}"
        );
        assert_eq!(
            hover("[bare]", doc).unwrap(),
            "**Link `[bare]`** → <https://x.io>"
        );
        assert!(hover("^note", doc).unwrap().contains("Some text."));
        assert!(hover("note", doc).unwrap().contains("Footnote"));
        assert!(
            hover("code", doc).is_none(),
            "definitions in code blocks don't count"
        );
        assert!(hover("missing", doc).is_none());
        assert!(hover("", doc).is_none());
    }

    #[test]
    fn ffi_round_trip() {
        unsafe {
            let src = CString::new("# T\n*e* \"q\"").unwrap();
            let out = tokenize_document_ffi(src.as_ptr());
            let json = CStr::from_ptr(out).to_str().unwrap().to_string();
            free_string(out);
            assert_eq!(
                json,
                r##"[[{"text":"# T","kind":"keyword"}],[{"text":"*e*","kind":"type"},{"text":" \"q\"","kind":"normal"}]]"##
            );

            let line = CString::new("**b**").unwrap();
            let out = tokenize_line_ffi(line.as_ptr());
            assert_eq!(
                CStr::from_ptr(out).to_str().unwrap(),
                r#"[{"text":"**b**","kind":"function"}]"#
            );
            free_string(out);

            let word = CString::new("a").unwrap();
            let doc = CString::new("[a]: https://a.io").unwrap();
            let out = hover_info_ffi(word.as_ptr(), doc.as_ptr());
            assert!(!out.is_null());
            free_string(out);
            assert!(hover_info_ffi(word.as_ptr(), std::ptr::null()).is_null());

            assert_eq!(CStr::from_ptr(language_id()).to_str().unwrap(), "markdown");
            assert!(CStr::from_ptr(file_extensions())
                .to_str()
                .unwrap()
                .starts_with("md,markdown"));
            free_string(std::ptr::null_mut());
        }
    }
}
