use std::sync::OnceLock;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute", "comment", "comment.documentation", "constant", "constant.builtin",
    "constructor", "function", "function.builtin", "function.macro", "function.method",
    "keyword", "keyword.function", "keyword.operator", "keyword.return", "keyword.storage",
    "label", "number", "operator", "property", "punctuation", "punctuation.bracket",
    "punctuation.delimiter", "string", "string.escape", "string.special",
    "type", "type.builtin", "variable", "variable.builtin", "variable.parameter",
    "tag",
];

fn capture_to_kind(idx: usize) -> &'static str {
    match idx {
        0 | 8 => "macro",
        1 | 2 => "comment",
        3 | 4 | 10 | 11 | 13 | 14 | 28 => "keyword",
        5 | 25 | 26 => "type",
        6 | 7 | 9 => "function",
        16 => "number",
        18 => "property",
        22 | 23 | 24 => "string",
        30 => "keyword",
        _ => "normal",
    }
}

static CONFIG_TS: OnceLock<HighlightConfiguration> = OnceLock::new();
static CONFIG_TSX: OnceLock<HighlightConfiguration> = OnceLock::new();

/// The TypeScript queries only cover what TypeScript adds to JavaScript
/// (types, `interface`, `declare`…): the grammar inherits the JavaScript
/// ones, so both are joined. The later query wins on a node both capture,
/// so JavaScript comes first and JSX (TSX only) last.
fn highlights_query(tsx: bool) -> String {
    let mut parts = vec![
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
    ];
    if tsx {
        parts.push(tree_sitter_javascript::JSX_HIGHLIGHT_QUERY);
    }
    parts.join("\n")
}

fn locals_query() -> String {
    [
        tree_sitter_typescript::LOCALS_QUERY,
        tree_sitter_javascript::LOCALS_QUERY,
    ]
    .join("\n")
}

fn get_config(tsx: bool) -> &'static HighlightConfiguration {
    if tsx {
        CONFIG_TSX.get_or_init(|| {
            let mut cfg = HighlightConfiguration::new(
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                "tsx",
                &highlights_query(tsx),
                "",
                &locals_query(),
            )
            .expect("tree-sitter-typescript TSX config");
            cfg.configure(HIGHLIGHT_NAMES);
            cfg
        })
    } else {
        CONFIG_TS.get_or_init(|| {
            let mut cfg = HighlightConfiguration::new(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                "typescript",
                &highlights_query(tsx),
                "",
                &locals_query(),
            )
            .expect("tree-sitter-typescript config");
            cfg.configure(HIGHLIGHT_NAMES);
            cfg
        })
    }
}

thread_local! {
    static HL: std::cell::RefCell<Highlighter> =
        std::cell::RefCell::new(Highlighter::new());
}

fn document_to_json(source: &str, tsx: bool) -> String {
    let config = get_config(tsx);
    let source_bytes = source.as_bytes();
    let num_lines = source.lines().count().max(1);

    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(source_bytes.iter().enumerate().filter_map(|(i, &b)| {
            if b == b'\n' { Some(i + 1) } else { None }
        }))
        .collect();

    let events: Vec<HighlightEvent> = HL.with(|cell| -> Result<Vec<HighlightEvent>, _> {
        let mut hl = cell.borrow_mut();
        let result = hl.highlight(config, source_bytes, None, |_| None)?
            .collect::<Result<Vec<_>, _>>();
        result
    })
    .unwrap_or_default();

    let mut line_tokens: Vec<Vec<(String, &'static str)>> = vec![Vec::new(); num_lines];
    let mut kind_stack: Vec<&'static str> = Vec::new();

    for event in events {
        match event {
            HighlightEvent::HighlightStart(h) => kind_stack.push(capture_to_kind(h.0)),
            HighlightEvent::HighlightEnd => { kind_stack.pop(); }
            HighlightEvent::Source { start, end } => {
                if start >= end { continue; }
                let kind = kind_stack.last().copied().unwrap_or("normal");
                let text = match source.get(start..end) {
                    Some(t) => t,
                    None => continue,
                };
                let start_line =
                    line_starts.partition_point(|&s| s <= start).saturating_sub(1);
                let mut line_idx = start_line;
                for piece in text.split('\n') {
                    if line_idx < line_tokens.len() && !piece.is_empty() {
                        line_tokens[line_idx].push((piece.to_string(), kind));
                    }
                    line_idx += 1;
                }
            }
        }
    }

    for (toks, line_text) in line_tokens.iter_mut().zip(source.lines()) {
        if toks.is_empty() && !line_text.is_empty() {
            toks.push((line_text.to_string(), "normal"));
        }
    }

    serialize_lines(&line_tokens)
}

fn serialize_lines(lines: &[Vec<(String, &'static str)>]) -> String {
    let lines_json: Vec<String> = lines
        .iter()
        .map(|toks| {
            let tok_strs: Vec<String> = toks
                .iter()
                .map(|(text, kind)| {
                    format!(r#"{{"text":{},"kind":"{}"}}"#, json_escape(text), kind)
                })
                .collect();
            format!("[{}]", tok_strs.join(","))
        })
        .collect();
    format!("[{}]", lines_json.join(","))
}

fn json_escape(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

// ── FFI ───────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const std::ffi::c_char {
    c"typescript".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const std::ffi::c_char {
    c"ts,tsx,mts,cts".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// Tokenize the entire document.
/// The filename hint (first line of the text pointer, terminated by `\n`) is not
/// used here — TSX grammar is selected when the extension contains "x".
/// Caller supplies just the source text; extension routing is handled by the IDE.
///
/// Returns a JSON `[[{text,kind},...],...]` array. Caller must free with `free_string`.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(
    text_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let text = unsafe { std::ffi::CStr::from_ptr(text_ptr).to_str().unwrap_or("") };
    let json = document_to_json(text, false);
    std::ffi::CString::new(json).unwrap_or_default().into_raw()
}

/// TSX variant — called by the IDE for .tsx / .jsx files.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_tsx_ffi(
    text_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let text = unsafe { std::ffi::CStr::from_ptr(text_ptr).to_str().unwrap_or("") };
    let json = document_to_json(text, true);
    std::ffi::CString::new(json).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn tokenize_line_ffi(
    _line_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    std::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut std::ffi::c_char) {
    if !ptr.is_null() {
        unsafe { drop(std::ffi::CString::from_raw(ptr)) }
    }
}

// ── Hover info ────────────────────────────────────────────────────────────────

pub fn hover_info(word: &str, file_content: &str) -> Option<String> {
    let fn_patterns = [
        format!("function {}(", word),
        format!("function {} (", word),
        format!("async function {}(", word),
        format!("async function {} (", word),
    ];
    let arrow_patterns = [
        format!("const {} = (", word),
        format!("const {} = async (", word),
        format!("let {} = (", word),
        format!("var {} = (", word),
    ];
    let decl_patterns = [
        format!("class {} ", word),
        format!("class {}{}", word, '{'),
        format!("interface {} ", word),
        format!("interface {}{}", word, '{'),
        format!("type {} =", word),
        format!("const {}", word),
        format!("let {}", word),
        format!("var {}", word),
    ];

    for line in file_content.lines() {
        let trimmed = line.trim();
        for pat in &fn_patterns {
            if trimmed.contains(pat.as_str()) {
                let sig = trimmed.trim_end_matches('{').trim_end();
                return Some(format!("```ts\n{sig}\n```"));
            }
        }
        for pat in &arrow_patterns {
            if trimmed.starts_with(pat.as_str()) {
                let sig = trimmed.trim_end_matches('{').trim_end();
                return Some(format!("```ts\n{sig}\n```"));
            }
        }
        for pat in &decl_patterns {
            if trimmed.starts_with(pat.as_str()) {
                let end = trimmed
                    .find('=')
                    .or_else(|| trimmed.find(';'))
                    .unwrap_or(trimmed.len());
                let sig = trimmed[..end].trim_end();
                return Some(format!("```ts\n{sig}\n```"));
            }
        }
    }
    None
}

#[no_mangle]
pub unsafe extern "C" fn hover_info_ffi(
    word_ptr: *const std::ffi::c_char,
    file_content_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let word = unsafe { std::ffi::CStr::from_ptr(word_ptr).to_str().unwrap_or("") };
    let content = unsafe { std::ffi::CStr::from_ptr(file_content_ptr).to_str().unwrap_or("") };
    match hover_info(word, content) {
        Some(s) => std::ffi::CString::new(s)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kind of the first token whose text is `text` in the JSON.
    fn kind_of(json: &str, text: &str) -> Option<String> {
        let needle = format!(r#"{{"text":{},"kind":""#, json_escape(text));
        let start = json.find(&needle)? + needle.len();
        let end = json[start..].find('"')? + start;
        Some(json[start..end].to_string())
    }

    #[test]
    fn javascript_syntax_is_highlighted_too() {
        let src = "// note\nconst s: string = \"hi\";\nfunction f(x: number) { return x + 1; }\n";
        let json = document_to_json(src, false);
        assert_eq!(kind_of(&json, "// note").as_deref(), Some("comment"));
        assert_eq!(kind_of(&json, "const").as_deref(), Some("keyword"));
        assert_eq!(kind_of(&json, "\"hi\"").as_deref(), Some("string"));
        assert_eq!(kind_of(&json, "f").as_deref(), Some("function"));
        assert_eq!(kind_of(&json, "return").as_deref(), Some("keyword"));
        assert_eq!(kind_of(&json, "1").as_deref(), Some("number"));
        assert_eq!(kind_of(&json, "number").as_deref(), Some("type"));
    }

    #[test]
    fn tsx_tags_are_highlighted() {
        let json = document_to_json("const a = <div className=\"x\">hi</div>;\n", true);
        assert_eq!(kind_of(&json, "div").as_deref(), Some("keyword"));
        assert_eq!(kind_of(&json, "\"x\"").as_deref(), Some("string"));
    }
}
