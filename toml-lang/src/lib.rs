use std::sync::OnceLock;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute", "comment", "comment.documentation", "constant", "constant.builtin",
    "constructor", "function", "function.builtin", "function.macro", "function.method",
    "keyword", "keyword.function", "keyword.operator", "keyword.return", "keyword.storage",
    "label", "number", "operator", "property", "punctuation", "punctuation.bracket",
    "punctuation.delimiter", "string", "string.escape", "string.special",
    "type", "type.builtin", "variable", "variable.builtin", "variable.parameter",
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
        _ => "normal",
    }
}

static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();

/// The grammar's highlight query, fixed: its `(pair (bare_key)) @property`
/// captures the whole `key = value` pair (the key's color ran over the
/// spaces, and over `true` / `false`), and `@boolean` isn't one of our colors.
fn highlights_query() -> String {
    const WHOLE_PAIR: &str = "(pair
  (bare_key)) @property";
    let query = tree_sitter_toml_ng::HIGHLIGHTS_QUERY;
    assert!(query.contains(WHOLE_PAIR), "tree-sitter-toml-ng query changed");
    format!(
        "{}
(boolean) @constant.builtin
",
        query.replacen(WHOLE_PAIR, "(pair
  (bare_key) @property)", 1)
    )
}

fn get_config() -> &'static HighlightConfiguration {
    CONFIG.get_or_init(|| {
        let mut cfg = HighlightConfiguration::new(
            tree_sitter_toml_ng::LANGUAGE.into(),
            "toml",
            &highlights_query(),
            "",
            "",
        )
        .expect("tree-sitter-toml config");
        cfg.configure(HIGHLIGHT_NAMES);
        cfg
    })
}

thread_local! {
    static HL: std::cell::RefCell<Highlighter> =
        std::cell::RefCell::new(Highlighter::new());
}

fn document_to_json(source: &str) -> String {
    let config = get_config();
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
    c"toml".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const std::ffi::c_char {
    c"toml".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(
    text_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let text = unsafe { std::ffi::CStr::from_ptr(text_ptr).to_str().unwrap_or("") };
    let json = document_to_json(text);
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

#[no_mangle]
pub unsafe extern "C" fn hover_info_ffi(
    _word_ptr: *const std::ffi::c_char,
    _file_content_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    std::ptr::null_mut()
}
