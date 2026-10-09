//! Docker module: Dockerfiles (`dockerfile`, `containerfile`) and Compose
//! files (`compose`), plus a Docker images panel in the IDE.
//!
//! The IDE passes the language to the `*_lang_ffi` exports; the older
//! language-less exports guess it from the document (Compose files are YAML
//! mappings, Dockerfiles start with an instruction).

use std::ffi::{c_char, CStr, CString};

pub mod compose;
pub mod containers;
pub mod dockerfile;
pub mod panel;

/// Tokens of each line of `source` in `lang`, as (text, kind name).
pub fn tokenize(lang: &str, source: &str) -> Vec<Vec<(String, &'static str)>> {
    fn names<K: Copy>(
        lines: Vec<Vec<(String, K)>>,
        name: fn(K) -> &'static str,
    ) -> Vec<Vec<(String, &'static str)>> {
        lines
            .into_iter()
            .map(|l| l.into_iter().map(|(t, k)| (t, name(k))).collect())
            .collect()
    }
    if lang == "compose" {
        names(compose::tokenize(source), compose::Kind::as_str)
    } else {
        names(dockerfile::tokenize(source), dockerfile::Kind::as_str)
    }
}

pub fn hover(lang: &str, word: &str, content: &str) -> Option<String> {
    if lang == "compose" {
        compose::hover(word, content)
    } else {
        dockerfile::hover(word, content)
    }
}

/// Language of a document when the IDE does not say: `compose` when its
/// first meaningful line is a YAML key (`services:`), else `dockerfile`.
pub fn guess_language(text: &str) -> &'static str {
    let first = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#') && *l != "---");
    match first {
        Some(l) if dockerfile::is_instruction_line(l) => "dockerfile",
        Some(l) if l.split_once(':').is_some_and(|(k, _)| !k.contains(' ')) => "compose",
        _ => "dockerfile",
    }
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

fn line_to_json(tokens: &[(String, &'static str)]) -> String {
    let toks: Vec<String> = tokens
        .iter()
        .map(|(text, kind)| format!(r#"{{"text":{},"kind":"{kind}"}}"#, json_escape(text)))
        .collect();
    format!("[{}]", toks.join(","))
}

fn document_to_json(lang: &str, source: &str) -> String {
    let lines: Vec<String> = tokenize(lang, source)
        .iter()
        .map(|l| line_to_json(l))
        .collect();
    format!("[{}]", lines.join(","))
}

fn first_line_json(lang: &str, line: &str) -> String {
    line_to_json(&tokenize(lang, line).into_iter().next().unwrap_or_default())
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

fn opt_into_c(s: Option<String>) -> *mut c_char {
    s.map(into_c).unwrap_or(std::ptr::null_mut())
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
    c"dockerfile".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"dockerfile,containerfile,compose".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// # Safety
/// Both pointers must be null or valid NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_lang_ffi(
    lang_ptr: *const c_char,
    text_ptr: *const c_char,
) -> *mut c_char {
    let (lang, text) = unsafe { (from_c(lang_ptr), from_c(text_ptr)) };
    into_c(document_to_json(lang, text))
}

/// # Safety
/// Both pointers must be null or valid NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn tokenize_line_lang_ffi(
    lang_ptr: *const c_char,
    line_ptr: *const c_char,
) -> *mut c_char {
    let (lang, line) = unsafe { (from_c(lang_ptr), from_c(line_ptr)) };
    into_c(first_line_json(lang, line))
}

/// # Safety
/// All pointers must be null or valid NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn hover_info_lang_ffi(
    lang_ptr: *const c_char,
    word_ptr: *const c_char,
    content_ptr: *const c_char,
) -> *mut c_char {
    let (lang, word, content) =
        unsafe { (from_c(lang_ptr), from_c(word_ptr), from_c(content_ptr)) };
    opt_into_c(hover(lang, word, content))
}

/// Language-less variant for IDEs before 0.10.7: the language is guessed.
///
/// # Safety
/// `text_ptr` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    let text = unsafe { from_c(text_ptr) };
    into_c(document_to_json(guess_language(text), text))
}

/// # Safety
/// `line_ptr` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_line_ffi(line_ptr: *const c_char) -> *mut c_char {
    let line = unsafe { from_c(line_ptr) };
    into_c(first_line_json("dockerfile", line))
}

/// # Safety
/// Both pointers must be null or valid NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn hover_info_ffi(
    word_ptr: *const c_char,
    file_content_ptr: *const c_char,
) -> *mut c_char {
    let (word, content) = unsafe { (from_c(word_ptr), from_c(file_content_ptr)) };
    opt_into_c(hover(guess_language(content), word, content))
}

/// JSON view of a panel declared in `[[panels]]` (see [`panel`]).
///
/// # Safety
/// `panel_ptr` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn ui_view_ffi(panel_ptr: *const c_char) -> *mut c_char {
    let panel = unsafe { from_c(panel_ptr) };
    opt_into_c(panel::view(panel).or_else(|| containers::view(panel)))
}

/// Handle a panel event; returns the JSON actions for the IDE.
///
/// # Safety
/// Both pointers must be null or valid NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn ui_event_ffi(
    panel_ptr: *const c_char,
    event_ptr: *const c_char,
) -> *mut c_char {
    let (panel, event) = unsafe { (from_c(panel_ptr), from_c(event_ptr)) };
    opt_into_c(panel::event(panel, event).or_else(|| containers::event(panel, event)))
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

    fn call(f: impl FnOnce() -> *mut c_char) -> Option<String> {
        let out = f();
        if out.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_string();
        unsafe { free_string(out) };
        Some(s)
    }

    #[test]
    fn language_is_routed_to_its_tokenizer() {
        let lang = CString::new("compose").unwrap();
        let text = CString::new("services:\n  web: {}").unwrap();
        let json = call(|| unsafe { tokenize_document_lang_ffi(lang.as_ptr(), text.as_ptr()) });
        assert!(json
            .unwrap()
            .starts_with(r#"[[{"text":"services","kind":"property"}"#));

        let lang = CString::new("dockerfile").unwrap();
        let text = CString::new("FROM x").unwrap();
        let json = call(|| unsafe { tokenize_line_lang_ffi(lang.as_ptr(), text.as_ptr()) });
        assert!(json
            .unwrap()
            .starts_with(r#"[{"text":"FROM","kind":"keyword"}"#));
    }

    #[test]
    fn language_less_exports_guess_it() {
        assert_eq!(guess_language("# c\nFROM rust AS b"), "dockerfile");
        assert_eq!(guess_language("---\nservices:\n  a: {}"), "compose");
        assert_eq!(guess_language("name: x"), "compose");
        assert_eq!(guess_language("RUN echo a: b"), "dockerfile");
        assert_eq!(guess_language(""), "dockerfile");
        let text = CString::new("services:\n  web:").unwrap();
        let json = call(|| unsafe { tokenize_document_ffi(text.as_ptr()) }).unwrap();
        assert!(json.contains(r#""kind":"property""#), "{json}");
    }

    #[test]
    fn hover_by_language() {
        let doc = "services:\n  web:\n    image: x\n";
        assert!(hover("compose", "image", doc).is_some());
        assert!(hover("dockerfile", "from", "").is_some());
        let (word, content) = (CString::new("image").unwrap(), CString::new(doc).unwrap());
        assert!(call(|| unsafe { hover_info_ffi(word.as_ptr(), content.as_ptr()) }).is_some());
    }

    #[test]
    fn panel_exports_answer_for_declared_panels_only() {
        let other = CString::new("other").unwrap();
        assert!(call(|| unsafe { ui_view_ffi(other.as_ptr()) }).is_none());
        let page = CString::new(panel::PAGE).unwrap();
        let ev = CString::new(r#"{"type":"click","id":"open_page"}"#).unwrap();
        let actions = call(|| unsafe { ui_event_ffi(page.as_ptr(), ev.as_ptr()) }).unwrap();
        assert!(actions.contains("open_panel"), "{actions}");
    }
}
