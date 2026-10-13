//! YAML highlighting with tree-sitter.

use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;

fn config() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        unicorns_highlight::configure(
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
        )
    })
}

/// The document as the IDE's token lines.
pub fn document_to_json(source: &str) -> String {
    unicorns_highlight::document(config(), source)
}

// ── FFI ───────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const c_char {
    c"yaml".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"yml,yaml".as_ptr()
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
        let json = document_to_json("# deploy\nname: CI\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    retries: 3\n    enabled: true\n    anchor: &base { a: \"x\" }\n");
        for (text, kind) in [
            ("# deploy", "comment"),
            ("name", "property"),
            ("CI", "string"),
            ("3", "number"),
            ("true", "keyword"),
            ("\"x\"", "string"),
        ] {
            let needle = format!(r#"{{"text":"{}","kind":"{kind}"}}"#, text.replace('"', "\\\""));
            assert!(json.contains(&needle), "{text} as {kind} in {json}");
        }
    }

    #[test]
    fn ffi_round_trip() {
        let src = CString::new("# deploy\nname: CI\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    retries: 3\n    enabled: true\n    anchor: &base { a: \"x\" }\n").unwrap();
        let out = unsafe { tokenize_document_ffi(src.as_ptr()) };
        assert!(!out.is_null());
        let json = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_string();
        unsafe { free_string(out) };
        assert!(json.starts_with("[[") && json.ends_with("]]"));
    }
}
