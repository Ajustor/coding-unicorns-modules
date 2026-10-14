//! Shell script highlighting (sh, Bash, Zsh) with tree-sitter-bash.

use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;

fn config() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        // The grammar's query leaves numbers and the declaration commands
        // (`readonly`, `local`…) uncolored. Appended: the last pattern
        // matching a node wins.
        let query = format!(
            "{}\n(number) @number\n[\"declare\" \"local\" \"readonly\" \"typeset\" \"unset\"] @keyword\n",
            tree_sitter_bash::HIGHLIGHT_QUERY
        );
        unicorns_highlight::configure(tree_sitter_bash::LANGUAGE.into(), "bash", &query, "")
    })
}

/// The document as the IDE's token lines.
pub fn document_to_json(source: &str) -> String {
    unicorns_highlight::document(config(), source)
}

// ── FFI ───────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const c_char {
    c"sh".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"sh,bash,zsh".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// Tokenize the whole document: `[[{text,kind},…],…]`, freed with
/// [`free_string`].
///
/// # Safety
/// `text_ptr` must be null or a valid, nul-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    if text_ptr.is_null() {
        return std::ptr::null_mut();
    }
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
