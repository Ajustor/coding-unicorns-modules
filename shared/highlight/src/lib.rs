//! Tree-sitter highlighting shared by the language modules: a grammar run
//! over a whole document, or a component (Svelte, Vue) whose template,
//! script, style and template expressions each get their own grammar.
//!
//! Every pass paints a kind per byte of the source; [`Paint::to_json`] then
//! groups them into the `[[{"text","kind"}…]…]` lines the IDE reads.

use std::cell::RefCell;
use std::ops::Range;
use std::sync::OnceLock;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// The token kinds the IDE colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    Keyword,
    Type,
    String,
    Comment,
    Number,
    Function,
    Macro,
    Property,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Keyword => "keyword",
            Self::Type => "type",
            Self::String => "string",
            Self::Comment => "comment",
            Self::Number => "number",
            Self::Function => "function",
            Self::Macro => "macro",
            Self::Property => "property",
        }
    }
}

/// Capture names the configurations recognise. tree-sitter-highlight maps a
/// capture to the longest listed prefix (`function.call` → `function`), so
/// this covers the queries of every grammar used.
const NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "character",
    "comment",
    "conditional",
    "constant",
    "constant.builtin",
    "constructor",
    "escape",
    "exception",
    "field",
    "float",
    "function",
    "include",
    "keyword",
    "label",
    "method",
    "module",
    "namespace",
    "number",
    "operator",
    "property",
    "punctuation",
    "repeat",
    "storageclass",
    "string",
    "tag",
    "type",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

fn kind_of_capture(index: usize) -> Kind {
    match NAMES.get(index).copied().unwrap_or("") {
        "comment" => Kind::Comment,
        "string" | "escape" | "character" => Kind::String,
        "number" | "float" => Kind::Number,
        "boolean" | "conditional" | "constant" | "constant.builtin" | "exception"
        | "include" | "keyword" | "repeat" | "storageclass" | "tag" | "variable.builtin" => {
            Kind::Keyword
        }
        "type" | "constructor" | "module" | "namespace" => Kind::Type,
        "function" | "method" => Kind::Function,
        "attribute" | "field" | "property" => Kind::Property,
        _ => Kind::Normal,
    }
}

/// A highlight configuration recognising [`NAMES`].
pub fn configure(
    language: tree_sitter::Language,
    name: &str,
    highlights: &str,
    locals: &str,
) -> HighlightConfiguration {
    let mut config = HighlightConfiguration::new(language, name, highlights, "", locals)
        .unwrap_or_else(|e| panic!("{name} highlight query: {e}"));
    config.configure(NAMES);
    config
}

/// TypeScript, which also parses JavaScript. Its own query only covers what
/// TypeScript adds: the JavaScript one comes first (the later query wins on a
/// node both capture).
pub fn typescript() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let highlights = [
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        ]
        .join("\n");
        let locals = [
            tree_sitter_typescript::LOCALS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ]
        .join("\n");
        configure(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            &highlights,
            &locals,
        )
    })
}

pub fn css() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        configure(
            tree_sitter_css::LANGUAGE.into(),
            "css",
            tree_sitter_css::HIGHLIGHTS_QUERY,
            "",
        )
    })
}

/// HTML, attribute values with their quotes.
pub fn html() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let highlights = format!(
            "(quoted_attribute_value) @string\n{}",
            tree_sitter_html::HIGHLIGHTS_QUERY
        );
        configure(tree_sitter_html::LANGUAGE.into(), "html", &highlights, "")
    })
}

thread_local! {
    static HIGHLIGHTER: RefCell<Highlighter> = RefCell::new(Highlighter::new());
}

/// The kind of every byte of a source, painted pass after pass.
pub struct Paint<'a> {
    source: &'a str,
    kinds: Vec<Kind>,
}

impl<'a> Paint<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            kinds: vec![Kind::Normal; source.len()],
        }
    }

    pub fn kind_at(&self, byte: usize) -> Kind {
        self.kinds.get(byte).copied().unwrap_or(Kind::Normal)
    }

    pub fn fill(&mut self, range: Range<usize>, kind: Kind) {
        let end = range.end.min(self.kinds.len());
        if range.start < end {
            self.kinds[range.start..end].fill(kind);
        }
    }

    /// Highlight `range` of the source with `config`, replacing what an
    /// earlier pass painted there.
    pub fn highlight(&mut self, config: &HighlightConfiguration, range: Range<usize>) {
        let Some(text) = self.source.get(range.clone()) else {
            return;
        };
        self.fill(range.clone(), Kind::Normal);
        let events = HIGHLIGHTER.with(|h| {
            h.borrow_mut()
                .highlight(config, text.as_bytes(), None, |_| None)
                .and_then(|events| events.collect::<Result<Vec<_>, _>>())
                .unwrap_or_default()
        });
        let mut stack: Vec<Kind> = Vec::new();
        for event in events {
            match event {
                HighlightEvent::HighlightStart(h) => stack.push(kind_of_capture(h.0)),
                HighlightEvent::HighlightEnd => {
                    stack.pop();
                }
                HighlightEvent::Source { start, end } => {
                    let kind = stack.last().copied().unwrap_or(Kind::Normal);
                    self.fill(range.start + start..range.start + end, kind);
                }
            }
        }
    }

    /// The painted source as the IDE's token lines.
    pub fn to_json(&self) -> String {
        let line_count = self.source.lines().count().max(1);
        let mut out = String::with_capacity(self.source.len() * 2 + 16);
        out.push('[');
        let mut start = 0;
        for (n, line) in self.source.split('\n').take(line_count).enumerate() {
            if n > 0 {
                out.push(',');
            }
            let end = start + line.strip_suffix('\r').unwrap_or(line).len();
            self.push_line(&mut out, start, end);
            start += line.len() + 1;
        }
        out.push(']');
        out
    }

    fn push_line(&self, out: &mut String, start: usize, end: usize) {
        out.push('[');
        let mut run_start = start;
        let mut first = true;
        let line = &self.source[start..end];
        let mut boundaries = line
            .char_indices()
            .map(|(i, _)| start + i)
            .chain(std::iter::once(end))
            .peekable();
        while let Some(i) = boundaries.next() {
            let next_kind = (i < end).then(|| self.kinds[i]);
            if i > run_start && next_kind != Some(self.kinds[run_start]) {
                if !first {
                    out.push(',');
                }
                first = false;
                push_token(out, &self.source[run_start..i], self.kinds[run_start]);
                run_start = i;
            }
            if boundaries.peek().is_none() {
                break;
            }
        }
        out.push(']');
    }
}

fn push_token(out: &mut String, text: &str, kind: Kind) {
    out.push_str("{\"text\":\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push_str("\",\"kind\":\"");
    out.push_str(kind.name());
    out.push_str("\"}");
}

/// A whole document highlighted with one grammar, as token lines.
pub fn document(config: &HighlightConfiguration, source: &str) -> String {
    let mut paint = Paint::new(source);
    paint.highlight(config, 0..source.len());
    paint.to_json()
}

/// Single-file component formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    /// `{expression}`, `{#if}…{/if}` blocks, `on:` / `bind:` directives.
    Svelte,
    /// `{{ expression }}`, `v-…` / `:prop` / `@event` / `#slot` directives.
    Vue,
}

/// A component highlighted: its markup as HTML, `<script>` as TypeScript,
/// `<style>` as CSS and its template expressions as TypeScript.
pub fn component(source: &str, flavor: Component) -> String {
    let mut paint = Paint::new(source);
    paint.highlight(html(), 0..source.len());
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .is_err()
    {
        return paint.to_json();
    }
    let Some(tree) = parser.parse(source, None) else {
        return paint.to_json();
    };
    let mut embedded = Vec::new();
    walk(tree.root_node(), source, flavor, &mut paint, &mut embedded);
    embedded.sort_by_key(|r| r.start);
    match flavor {
        Component::Svelte => svelte_expressions(source, &mut paint, &embedded),
        Component::Vue => vue_interpolations(source, &mut paint, &embedded),
    }
    paint.to_json()
}

/// Highlight the script and style blocks and the directive attributes;
/// collect the blocks' ranges.
fn walk(
    node: tree_sitter::Node,
    source: &str,
    flavor: Component,
    paint: &mut Paint,
    embedded: &mut Vec<Range<usize>>,
) {
    match node.kind() {
        "script_element" | "style_element" => {
            let mut cursor = node.walk();
            if let Some(raw) = node.children(&mut cursor).find(|c| c.kind() == "raw_text") {
                let config = if node.kind() == "script_element" {
                    typescript()
                } else {
                    css()
                };
                paint.highlight(config, raw.byte_range());
                embedded.push(raw.byte_range());
            }
            return;
        }
        "attribute" => directive(node, source, flavor, paint),
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, flavor, paint, embedded);
    }
}

/// Svelte `on:click` / `bind:value`: the directive in macro colour. Vue
/// `v-if` / `:prop` / `@event` / `#slot`: the name in macro colour and its
/// value as an expression.
fn directive(attribute: tree_sitter::Node, source: &str, flavor: Component, paint: &mut Paint) {
    let mut cursor = attribute.walk();
    let children: Vec<_> = attribute.children(&mut cursor).collect();
    let Some(name) = children.iter().find(|c| c.kind() == "attribute_name") else {
        return;
    };
    let text = &source[name.byte_range()];
    match flavor {
        Component::Svelte => {
            if let Some(colon) = text.find(':') {
                paint.fill(name.start_byte()..name.start_byte() + colon, Kind::Macro);
            }
        }
        Component::Vue => {
            if !(text.starts_with("v-")
                || text.starts_with(':')
                || text.starts_with('@')
                || text.starts_with('#'))
            {
                return;
            }
            paint.fill(name.byte_range(), Kind::Macro);
            for value in children.iter().filter(|c| c.kind() == "quoted_attribute_value") {
                let mut cursor = value.walk();
                let inner = value
                    .children(&mut cursor)
                    .find(|c| c.kind() == "attribute_value");
                if let Some(inner) = inner {
                    paint.highlight(typescript(), inner.byte_range());
                }
            }
        }
    }
}

/// Whether `i` is inside a script or style block, and where that block ends.
fn embedded_end(embedded: &[Range<usize>], i: usize) -> Option<usize> {
    embedded.iter().find(|r| r.contains(&i)).map(|r| r.end)
}

/// The `}` closing the `{` at `open`, skipping nested braces and strings.
fn closing_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 1;
                } else if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' | b'`' => quote = Some(b),
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Svelte `{expression}` and `{#block …}` / `{:else}` / `{/block}` /
/// `{@html …}` tags, in the markup and in attribute values.
fn svelte_expressions(source: &str, paint: &mut Paint, embedded: &[Range<usize>]) {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = embedded_end(embedded, i) {
            i = end;
            continue;
        }
        if bytes[i] != b'{' || paint.kind_at(i) == Kind::Comment {
            i += 1;
            continue;
        }
        let Some(close) = closing_brace(bytes, i) else {
            break;
        };
        let mut expr = i + 1;
        if matches!(bytes.get(expr), Some(b'#' | b'/' | b':' | b'@')) {
            // The block's sigil and keyword: `{#each`, `{:else`, `{/if`.
            expr += 1;
            while expr < close && (bytes[expr].is_ascii_alphanumeric() || bytes[expr] == b'_') {
                expr += 1;
            }
            paint.fill(i..expr, Kind::Macro);
            paint.fill(close..close + 1, Kind::Macro);
        } else {
            paint.fill(i..i + 1, Kind::Normal);
            paint.fill(close..close + 1, Kind::Normal);
        }
        if expr < close {
            paint.highlight(typescript(), expr..close);
        }
        i = close + 1;
    }
}

/// Vue `{{ expression }}` interpolations in the markup.
fn vue_interpolations(source: &str, paint: &mut Paint, embedded: &[Range<usize>]) {
    let mut i = 0;
    while let Some(found) = source[i..].find("{{") {
        let open = i + found;
        if let Some(end) = embedded_end(embedded, open) {
            i = end;
            continue;
        }
        let Some(close) = source[open + 2..].find("}}").map(|c| open + 2 + c) else {
            break;
        };
        if paint.kind_at(open) != Kind::Comment {
            paint.fill(open..open + 2, Kind::Macro);
            paint.fill(close..close + 2, Kind::Macro);
            paint.highlight(typescript(), open + 2..close);
        }
        i = close + 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kind of the first token whose text is `text`.
    fn kind_of(json: &str, text: &str) -> Option<String> {
        let needle = format!(r#"{{"text":"{}","kind":""#, text.replace('"', "\\\""));
        let start = json.find(&needle)? + needle.len();
        let end = json[start..].find('"')? + start;
        Some(json[start..end].to_string())
    }

    #[track_caller]
    fn assert_kinds(json: &str, expected: &[(&str, &str)]) {
        for (text, kind) in expected {
            assert_eq!(
                kind_of(json, text).as_deref(),
                Some(*kind),
                "{text:?} in {json}"
            );
        }
    }

    #[test]
    fn json_lines_match_the_source_lines() {
        let json = document(typescript(), "let a = 1;\r\n\r\n// \"q\"\n");
        assert_eq!(
            json,
            r#"[[{"text":"let","kind":"keyword"},{"text":" a = ","kind":"normal"},{"text":"1","kind":"number"},{"text":";","kind":"normal"}],[],[{"text":"// \"q\"","kind":"comment"}]]"#
        );
        assert_eq!(document(typescript(), ""), "[[]]");
        // Multi-byte text stays whole.
        assert_kinds(&document(typescript(), "const é = \"ü\";"), &[("\"ü\"", "string")]);
    }

    #[test]
    fn svelte_component() {
        let src = "<script lang=\"ts\">\n  let count: number = $state(0);\n  /* multi\n  line */\n</script>\n\n<!-- don't -->\n<p>Don't {count} if</p>\n{#if count > 1}\n  <button on:click={() => count++}>x</button>\n{:else}\n  <span class=\"a {count}\">n</span>\n{/if}\n\n<style>\n  p { color: red; }\n</style>\n";
        let json = component(src, Component::Svelte);
        assert_kinds(
            &json,
            &[
                ("script", "keyword"),
                ("let", "keyword"),
                ("number", "type"),
                ("0", "number"),
                ("  line */", "comment"),
                ("<!-- don't -->", "comment"),
                (">Don't {count} if</", "normal"),
                ("{#if", "macro"),
                ("1", "number"),
                ("on", "macro"),
                ("{:else}", "macro"),
                ("{/if}", "macro"),
                ("color", "property"),
            ],
        );
    }

    #[test]
    fn vue_component() {
        let src = "<template>\n  <div v-if=\"items.length > 0\" :class=\"cls\" @click=\"go(1)\">{{ msg.toUpperCase() }} and if</div>\n</template>\n<script setup lang=\"ts\">\nconst msg = 'hi'\n</script>\n<style scoped>\n.a { margin: 0 }\n</style>\n";
        let json = component(src, Component::Vue);
        assert_kinds(
            &json,
            &[
                ("template", "keyword"),
                ("v-if", "macro"),
                (":class", "macro"),
                ("@click", "macro"),
                ("0", "number"),
                ("go", "function"),
                ("{{", "macro"),
                ("toUpperCase", "function"),
                ("const", "keyword"),
                ("'hi'", "string"),
                ("margin", "property"),
            ],
        );
        // Prose is not code: "if" in the text stays plain.
        assert_kinds(&json, &[(" and if</", "normal")]);
    }
}
