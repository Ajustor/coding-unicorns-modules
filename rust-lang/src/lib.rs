use std::sync::OnceLock;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

// ── Highlight name → kind string mapping ─────────────────────────────────────

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",             // 0  → macro
    "comment",               // 1  → comment
    "comment.documentation", // 2  → comment
    "constant",              // 3  → keyword
    "constant.builtin",      // 4  → keyword
    "constructor",           // 5  → type
    "function",              // 6  → function
    "function.builtin",      // 7  → function
    "function.macro",        // 8  → macro
    "function.method",       // 9  → function
    "keyword",               // 10 → keyword
    "keyword.function",      // 11 → keyword
    "keyword.operator",      // 12 → normal
    "keyword.return",        // 13 → keyword
    "keyword.storage",       // 14 → keyword
    "label",                 // 15 → normal
    "number",                // 16 → number
    "operator",              // 17 → normal
    "property",              // 18 → property
    "punctuation",           // 19 → normal
    "punctuation.bracket",   // 20 → normal
    "punctuation.delimiter", // 21 → normal
    "string",                // 22 → string
    "string.escape",         // 23 → string
    "string.special",        // 24 → string
    "type",                  // 25 → type
    "type.builtin",          // 26 → type
    "variable",              // 27 → normal
    "variable.builtin",      // 28 → keyword
    "variable.parameter",    // 29 → normal
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

// ── Tree-sitter config (initialised once) ────────────────────────────────────

static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();

fn get_config() -> &'static HighlightConfiguration {
    CONFIG.get_or_init(|| {
        // The grammar's query paints numbers as `@constant.builtin`, like
        // `true`: ours, appended, wins (the last pattern for a node does).
        let query = format!(
            "{}
[(integer_literal) (float_literal)] @number
",
            tree_sitter_rust::HIGHLIGHTS_QUERY
        );
        let mut cfg = HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            &query,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        )
        .expect("tree-sitter-rust config");
        cfg.configure(HIGHLIGHT_NAMES);
        cfg
    })
}

thread_local! {
    static HL: std::cell::RefCell<Highlighter> =
        std::cell::RefCell::new(Highlighter::new());
}

// ── Document tokenizer ────────────────────────────────────────────────────────

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
    c"rust".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const std::ffi::c_char {
    c"rs".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// Tokenize the entire document. Returns a JSON array of lines, each line being
/// an array of `{"text":"...","kind":"..."}` objects. Caller must free with `free_string`.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(
    text_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let text = unsafe { std::ffi::CStr::from_ptr(text_ptr).to_str().unwrap_or("") };
    let json = document_to_json(text);
    std::ffi::CString::new(json).unwrap_or_default().into_raw()
}

/// Legacy single-line tokenizer — returns null so the IDE falls back to
/// `tokenize_document_ffi`. Kept for ABI compatibility.
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
        format!("fn {}(", word),
        format!("fn {} (", word),
        format!("pub fn {}(", word),
        format!("pub(crate) fn {}(", word),
        format!("async fn {}(", word),
        format!("pub async fn {}(", word),
        format!("unsafe fn {}(", word),
        format!("pub unsafe fn {}(", word),
    ];
    let type_patterns = [
        format!("struct {} ", word),
        format!("struct {}{}", word, '{'),
        format!("pub struct {}", word),
        format!("enum {} ", word),
        format!("pub enum {}", word),
        format!("trait {} ", word),
        format!("pub trait {}", word),
        format!("type {} =", word),
        format!("pub type {} =", word),
    ];
    let let_patterns = [
        format!("let {}: ", word),
        format!("let mut {}: ", word),
        format!("let {} =", word),
        format!("let mut {} =", word),
        format!("const {}: ", word),
        format!("static {}: ", word),
    ];

    for line in file_content.lines() {
        let trimmed = line.trim();
        for pat in &fn_patterns {
            if trimmed.contains(pat.as_str()) {
                let sig = trimmed.trim_end_matches('{').trim_end();
                return Some(format!("```rust\n{sig}\n```"));
            }
        }
        for pat in &type_patterns {
            if trimmed.contains(pat.as_str()) || trimmed.starts_with(pat.as_str()) {
                let sig = trimmed.trim_end_matches('{').trim_end();
                return Some(format!("```rust\n{sig}\n```"));
            }
        }
        for pat in &let_patterns {
            if trimmed.starts_with(pat.as_str()) {
                let end = trimmed
                    .find('=')
                    .or_else(|| trimmed.find(';'))
                    .unwrap_or(trimmed.len());
                let sig = trimmed[..end].trim_end();
                return Some(format!("```rust\n{sig}\n```"));
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
