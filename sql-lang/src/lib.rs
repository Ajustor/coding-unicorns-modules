//! SQL highlighting with tree-sitter.

use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;

fn config() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        // The query is written for Neovim: `%d` (a Lua pattern) in its
        // regexes, and a second `@spell` capture that hides the comments.
        let query = tree_sitter_sequel::HIGHLIGHTS_QUERY
            .replace("%d", "[0-9]")
            .replace(" @spell", "");
        unicorns_highlight::configure(tree_sitter_sequel::LANGUAGE.into(), "sql", &query, "")
    })
}

/// The document as the IDE's token lines.
pub fn document_to_json(source: &str) -> String {
    unicorns_highlight::document(config(), source)
}

// ── FFI ───────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const c_char {
    c"sql".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"sql".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// Tokenize the whole document: `[[{text,kind},…],…]`, freed with
/// [`free_string`].
///
/// # Safety
/// `text_ptr` must be a valid, nul-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    let text = unsafe { CStr::from_ptr(text_ptr) }.to_str().unwrap_or("");
    CString::new(document_to_json(text))
        .unwrap_or_default()
        .into_raw()
}

/// Line tokenizing is not offered: the IDE uses [`tokenize_document_ffi`].
#[no_mangle]
pub extern "C" fn tokenize_line_ffi(_line_ptr: *const c_char) -> *mut c_char {
    std::ptr::null_mut()
}

/// # Safety
/// `ptr` must come from this module and not be freed yet.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_a_sample() {
        let json = document_to_json("-- users\nSELECT id, count(*) AS n\nFROM users u\nWHERE u.age > 18 AND name = 'bob'\nGROUP BY id;\nCREATE TABLE t (id INTEGER PRIMARY KEY, label TEXT);\n");
        for (text, kind) in [
            ("-- users", "comment"),
            ("SELECT", "keyword"),
            ("count", "function"),
            ("18", "number"),
            ("'bob'", "string"),
            ("INTEGER", "type"),
        ] {
            let needle = format!(r#"{{"text":"{}","kind":"{kind}"}}"#, text.replace('"', "\\\""));
            assert!(json.contains(&needle), "{text} as {kind} in {json}");
        }
    }

    #[test]
    fn ffi_round_trip() {
        let src = CString::new("-- users\nSELECT id, count(*) AS n\nFROM users u\nWHERE u.age > 18 AND name = 'bob'\nGROUP BY id;\nCREATE TABLE t (id INTEGER PRIMARY KEY, label TEXT);\n").unwrap();
        let out = unsafe { tokenize_document_ffi(src.as_ptr()) };
        assert!(!out.is_null());
        let json = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_string();
        unsafe { free_string(out) };
        assert!(json.starts_with("[[") && json.ends_with("]]"));
    }
}
