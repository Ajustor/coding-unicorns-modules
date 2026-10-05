use std::sync::OnceLock;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const HIGHLIGHT_NAMES: &[&str] = &[
    "array", "assignvalue", "comment", "delimiter", "function", "keyword",
    "number", "operator", "property", "string", "type", "variable",
];

fn capture_to_kind(idx: usize) -> &'static str {
    match idx {
        2 => "comment",
        4 => "function",
        5 => "keyword",
        6 => "number",
        9 => "string",
        10 => "type",
        11 => "macro",
        _ => "normal",
    }
}

static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();

fn get_config() -> &'static HighlightConfiguration {
    CONFIG.get_or_init(|| {
        let mut cfg = HighlightConfiguration::new(
            tree_sitter_powershell::LANGUAGE.into(),
            "powershell",
            include_str!("../queries/highlights.scm"),
            "",
            "",
        )
        .expect("tree-sitter-powershell config");
        cfg.configure(HIGHLIGHT_NAMES);
        cfg
    })
}

thread_local! {
    static HL: std::cell::RefCell<Highlighter> =
        std::cell::RefCell::new(Highlighter::new());
}

fn document_to_json(source: &str) -> String {
    serialize_lines(&document_tokens(source))
}

fn document_tokens(source: &str) -> Vec<Vec<(String, &'static str)>> {
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
                    // CRLF files: keep the '\r' out of the rendered tokens.
                    let piece = piece.strip_suffix('\r').unwrap_or(piece);
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

    line_tokens
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
    c"powershell".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const std::ffi::c_char {
    c"ps1,psm1,psd1".as_ptr()
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

// ── Hover info ────────────────────────────────────────────────────────────────

/// Declaration line for `word` (function, filter, workflow, class, enum).
/// PowerShell identifiers are case-insensitive, so matching ignores case.
pub fn hover_info(word: &str, file_content: &str) -> Option<String> {
    if word.is_empty() {
        return None;
    }
    let word = word.to_lowercase();
    for line in file_content.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        let mut parts = lower.split_whitespace();
        let (Some(keyword), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !matches!(keyword, "function" | "filter" | "workflow" | "class" | "enum") {
            continue;
        }
        // `function Get-Foo {`, `function Get-Foo($a)`, `class Foo : Base`
        let name = name.split(['(', '{', ':']).next().unwrap_or("");
        if name == word {
            let sig = trimmed.trim_end_matches('{').trim_end();
            return Some(format!("```powershell\n{sig}\n```"));
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

    fn kinds_of(source: &str, text: &str) -> Vec<&'static str> {
        document_tokens(source)
            .into_iter()
            .flatten()
            .filter(|(t, _)| t == text)
            .map(|(_, k)| k)
            .collect()
    }

    #[test]
    fn test_highlight_names_match_kind_indices() {
        assert_eq!(HIGHLIGHT_NAMES[2], "comment");
        assert_eq!(HIGHLIGHT_NAMES[4], "function");
        assert_eq!(HIGHLIGHT_NAMES[5], "keyword");
        assert_eq!(HIGHLIGHT_NAMES[6], "number");
        assert_eq!(HIGHLIGHT_NAMES[9], "string");
        assert_eq!(HIGHLIGHT_NAMES[10], "type");
        assert_eq!(HIGHLIGHT_NAMES[11], "variable");
    }

    #[test]
    fn test_keywords_and_function_name() {
        let src = "function Get-Thing {\n    if ($x) { return 1 }\n}\n";
        assert!(kinds_of(src, "function").contains(&"keyword"));
        assert!(kinds_of(src, "if").contains(&"keyword"));
        assert!(kinds_of(src, "return").contains(&"keyword"));
        assert!(kinds_of(src, "Get-Thing").contains(&"function"));
        assert!(kinds_of(src, "1").contains(&"number"));
    }

    #[test]
    fn test_comment_string_variable_and_command() {
        let src = "# note\n$name = \"world\"\nWrite-Host $name\n";
        assert!(kinds_of(src, "# note").contains(&"comment"));
        assert!(kinds_of(src, "\"world\"").contains(&"string"));
        assert!(kinds_of(src, "$name").contains(&"macro"));
        assert!(kinds_of(src, "Write-Host").contains(&"function"));
    }

    #[test]
    fn test_type_literal() {
        let src = "[int]$n = 3\n";
        assert!(kinds_of(src, "int").contains(&"type"));
    }

    #[test]
    fn test_document_has_one_entry_per_line_and_strips_cr() {
        let src = "$a = 1\r\n# c\r\n";
        let lines = document_tokens(src);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().flatten().all(|(t, _)| !t.contains('\r')));
    }

    #[test]
    fn test_hover_function_is_case_insensitive() {
        let src = "function Get-Thing($Path) {\n}\n";
        assert_eq!(
            hover_info("get-thing", src).as_deref(),
            Some("```powershell\nfunction Get-Thing($Path)\n```")
        );
    }

    #[test]
    fn test_hover_class_and_enum() {
        let src = "class Animal : Base {\n}\nenum Color {\n  Red\n}\n";
        assert_eq!(
            hover_info("Animal", src).as_deref(),
            Some("```powershell\nclass Animal : Base\n```")
        );
        assert_eq!(
            hover_info("Color", src).as_deref(),
            Some("```powershell\nenum Color\n```")
        );
    }

    #[test]
    fn test_hover_unknown_or_empty_word() {
        let src = "function Foo {}\n";
        assert!(hover_info("Bar", src).is_none());
        assert!(hover_info("", src).is_none());
        // Not a prefix match.
        assert!(hover_info("Fo", src).is_none());
    }

    #[test]
    fn test_ffi_roundtrip() {
        let text = std::ffi::CString::new("$x = 1").unwrap();
        let ptr = unsafe { tokenize_document_ffi(text.as_ptr()) };
        let json = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
        unsafe { free_string(ptr) };
        assert!(json.starts_with("[["));
        assert!(json.contains(r#""kind":"number""#));

        let ext = unsafe { std::ffi::CStr::from_ptr(file_extensions()) };
        assert_eq!(ext.to_str().unwrap(), "ps1,psm1,psd1");
    }
}
